//! DB-backed saved-list coverage (Slice 011b). These tests deliberately use
//! the production command/read path for authority, revisions, retry identity,
//! and count semantics; direct SQL is limited to fixture setup and adversarial
//! stored JSONB rows that normal application writes cannot create.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Barrier;
use tokio::time::timeout;
use tower::ServiceExt;
use uuid::Uuid;

use crate::common::{
    add_membership_with, body_json, build_router, build_router_with_publisher, connect_as_app,
    create_org, create_org_with_stages_and_member, create_platform_admin, create_user,
    get_with_cookie, login_cookie, post_json_with_cookie, put_json_with_cookie,
};
use crm_api::auth::AuthContext;
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{
    AgeClause, AgeSpec, AssignedToClause, Assignee, BoolClause, Clause, FilterDefinition,
    SourceClause, StageClause,
};
use crm_api::domain::person::{queries as person_queries, PersonVisibilityScope};
use crm_api::domain::saved_list::{
    self, CreateSavedList, DeleteSavedList, SavedListError, SavedListScope, UpdateSavedList,
};
use crm_api::ids::{CorrelationId, OrganizationId, UserId};
use crm_api::realtime::Publisher;

const PW: &str = "correct horse battery staple";

fn empty_filter() -> FilterDefinition {
    FilterDefinition {
        version: 1,
        clauses: Vec::new(),
    }
}

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn auth_context(organization_id: Uuid, actor_user_id: Uuid, role: Role) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(actor_user_id),
        actor_email: "fixture@example.test".to_string(),
        actor_display_name: "Fixture".to_string(),
        active_organization_id: OrganizationId::new(organization_id),
        active_organization_name: "Fixture Organization".to_string(),
        role,
    }
}

async fn recorded(publisher: &Publisher) -> Vec<(String, Value)> {
    let Publisher::Recording(recorded, _) = publisher else {
        panic!("expected recording publisher");
    };
    recorded.lock().await.clone()
}

async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    request_id: Uuid,
    scope: SavedListScope,
    name: &str,
    filter: FilterDefinition,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id,
            scope,
            name: name.to_string(),
            filter,
        },
    )
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

async fn insert_people_batch(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    count: i64,
    assigned_user_id: Option<Uuid>,
) {
    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id, created_at)\
         SELECT $1, $2, $3, now() - make_interval(secs => source.i)\
         FROM generate_series(1, $4) AS source(i)",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_axis_person(
    pool: &PgPool,
    organization_id: Uuid,
    stage_id: Uuid,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, assigned_user_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(organization_id)
    .bind(stage_id)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_axis_facts(pool: &PgPool, organization_id: Uuid, person_id: Uuid, agent_id: Uuid) {
    // Two source rows pin the existing lateral "latest inquiry" semantics:
    // only the later zillow source must match a source clause.
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at)\
         VALUES ($1, $2, $3, 'old_source', now() - interval '2 days'),\
                ($1, $2, $4, 'zillow', now())",
    )
    .bind(organization_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO contact_attempted\
           (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,\
            person_id, channel, outcome)\
         VALUES ($1, 'system', NULL, 'migration', now(), $2, $3, 'call', 'reached')",
    )
    .bind(organization_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
    let raw_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO correspondence_raw\
           (id, organization_id, received_at, nonce, ciphertext, content_hmac, byte_len, processed)\
         VALUES ($1, $2, now(), $3, $4, $5, 0, true)",
    )
    .bind(raw_id)
    .bind(organization_id)
    .bind(vec![0_u8; 24])
    .bind(vec![1_u8; 16])
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO correspondence_captured\
           (organization_id, actor_kind, actor_user_id, on_behalf_of_user_id, origin,\
            occurred_at, correlation_id, person_id, agent_user_id, direction, via,\
            correspondence_raw_id, backdated)\
         VALUES ($1, 'system', NULL, NULL, 'migration', now(), $2, $3, $4,\
                 'inbound', 'cc', $5, false)",
    )
    .bind(organization_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(agent_id)
    .bind(raw_id)
    .execute(pool)
    .await
    .unwrap();
    for (kind, value) in [("phone", "+15551234567"), ("email", "axis@example.test")] {
        sqlx::query(
            "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)\
             VALUES ($1, $2, $3, $4, $4)",
        )
        .bind(organization_id)
        .bind(person_id)
        .bind(kind)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn assert_saved_count_parity(
    pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    filter: FilterDefinition,
    expected_count: i64,
) {
    let list = create_list(
        pool,
        organization_id,
        actor_user_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        &format!("Parity {}", Uuid::new_v4()),
        filter.clone(),
    )
    .await;
    let auth = auth_context(organization_id, actor_user_id, Role::Member);
    let scope = PersonVisibilityScope::from_auth(&auth);
    let params = filter.to_query_params(UserId::new(actor_user_id));
    let mut conn = pool.acquire().await.unwrap();
    let (summaries, summaries_truncated) =
        person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
    let count = saved_list::count_saved_list_matches(&mut conn, &auth, list.list.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.count, expected_count, "saved count expected value");
    assert_eq!(count.count as usize, summaries.len());
    assert_eq!(count.truncated, summaries_truncated);
}

async fn insert_live_seed_rows(
    pool: &PgPool,
    organization_id: Uuid,
    owner_id: Uuid,
    scope: SavedListScope,
    count: i64,
) {
    let scope = scope.as_str();
    sqlx::query(
        r#"INSERT INTO saved_list
             (organization_id, created_by_user_id, scope, name, filter,
              create_request_id, create_fingerprint)
           SELECT $1, $2, $3, 'seed ' || series.i::text,
                  '{"version":1,"clauses":[]}'::jsonb,
                  gen_random_uuid(), decode(repeat('00', 32), 'hex')
             FROM generate_series(1, $4) AS series(i)"#,
    )
    .bind(organization_id)
    .bind(owner_id)
    .bind(scope)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn delete_json(
    router: &axum::Router,
    uri: &str,
    cookie: &str,
    body: Value,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn post_raw(
    router: &axum::Router,
    uri: &str,
    cookie: &str,
    body: String,
) -> axum::response::Response {
    raw_request(router, "POST", uri, cookie, body).await
}

async fn raw_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: String,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// The privacy contract intentionally makes hidden, foreign, and missing
/// resources indistinguishable. Keep this byte-level rather than only a
/// JSON-value assertion: callers must not be able to distinguish those paths
/// from an envelope formatting difference either.
async fn assert_status_body(
    response: axum::response::Response,
    expected_status: StatusCode,
    expected_body: &[u8],
    label: &str,
) {
    assert_eq!(response.status(), expected_status, "{label}");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), expected_body, "{label}");
}

// --- Migration / grants ---------------------------------------------------

#[sqlx::test]
#[ignore]
async fn saved_list_schema_enforces_live_tombstone_shape_and_app_grants(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list schema",
        "owner@saved-list-schema.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let valid_insert = sqlx::query(
        r#"INSERT INTO saved_list
             (organization_id, created_by_user_id, scope, name, filter,
              create_request_id, create_fingerprint)
           VALUES ($1, $2, 'personal', 'Valid', '{"version":1,"clauses":[]}'::jsonb,
                   $3, decode(repeat('01', 32), 'hex'))"#,
    )
    .bind(organization_id)
    .bind(owner_id)
    .bind(Uuid::new_v4())
    .execute(&app_pool)
    .await;
    assert!(
        valid_insert.is_ok(),
        "crm_app receives INSERT on saved_list"
    );

    for (label, sql) in [
        (
            "invalid scope",
            "INSERT INTO saved_list (organization_id, created_by_user_id, scope, name, filter, create_request_id, create_fingerprint) VALUES ($1, $2, 'other', 'X', '{}'::jsonb, $3, decode(repeat('00', 32), 'hex'))",
        ),
        (
            "empty live name",
            "INSERT INTO saved_list (organization_id, created_by_user_id, scope, name, filter, create_request_id, create_fingerprint) VALUES ($1, $2, 'personal', '', '{}'::jsonb, $3, decode(repeat('00', 32), 'hex'))",
        ),
        (
            "array filter",
            "INSERT INTO saved_list (organization_id, created_by_user_id, scope, name, filter, create_request_id, create_fingerprint) VALUES ($1, $2, 'personal', 'X', '[]'::jsonb, $3, decode(repeat('00', 32), 'hex'))",
        ),
        (
            "live row missing name",
            "INSERT INTO saved_list (organization_id, created_by_user_id, scope, name, filter, create_request_id, create_fingerprint) VALUES ($1, $2, 'personal', NULL, '{}'::jsonb, $3, decode(repeat('00', 32), 'hex'))",
        ),
        (
            "live row missing filter",
            "INSERT INTO saved_list (organization_id, created_by_user_id, scope, name, filter, create_request_id, create_fingerprint) VALUES ($1, $2, 'personal', 'X', NULL, $3, decode(repeat('00', 32), 'hex'))",
        ),
        (
            "tombstone retains content",
            "INSERT INTO saved_list (organization_id, created_by_user_id, scope, name, filter, create_request_id, create_fingerprint, deleted_at) VALUES ($1, $2, 'personal', 'X', '{}'::jsonb, $3, decode(repeat('00', 32), 'hex'), now())",
        ),
    ] {
        let result = sqlx::query(sql)
            .bind(organization_id)
            .bind(owner_id)
            .bind(Uuid::new_v4())
            .execute(&migrator_pool)
            .await;
        assert!(result.is_err(), "{label} must be rejected by the schema");
    }

    let non_member_id = create_user(
        &migrator_pool,
        "not-a-member@saved-list-schema.test",
        "Not a member",
        PW,
    )
    .await;
    let foreign_creator = sqlx::query(
        r#"INSERT INTO saved_list
              (organization_id, created_by_user_id, scope, name, filter,
               create_request_id, create_fingerprint)
           VALUES ($1, $2, 'personal', 'Wrong creator',
                   '{"version":1,"clauses":[]}'::jsonb,
                   $3, decode(repeat('00', 32), 'hex'))"#,
    )
    .bind(organization_id)
    .bind(non_member_id)
    .bind(Uuid::new_v4())
    .execute(&migrator_pool)
    .await;
    assert!(
        foreign_creator.is_err(),
        "the composite organization/member FK must reject a non-member creator"
    );

    let delete = sqlx::query("DELETE FROM saved_list WHERE false")
        .execute(&app_pool)
        .await;
    let truncate = sqlx::query("TRUNCATE saved_list").execute(&app_pool).await;
    assert!(delete.is_err(), "crm_app must not hard-delete saved lists");
    assert!(truncate.is_err(), "crm_app must not truncate saved lists");
}

// --- Commands / retry / revisions ----------------------------------------

#[sqlx::test]
#[ignore]
async fn create_replay_edit_delete_and_direct_version_validation(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list lifecycle",
        "owner@saved-list-lifecycle.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let request_id = Uuid::new_v4();

    let created = create_list(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "  My list  ",
        empty_filter(),
    )
    .await;
    assert!(created.created);
    assert_eq!(created.list.name, "My list");

    let replay = create_list(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "My list",
        empty_filter(),
    )
    .await;
    assert!(!replay.created);
    assert_eq!(replay.list.id, created.list.id);

    let updated = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 1,
            name: "Renamed".to_string(),
            filter: empty_filter(),
        },
    )
    .await
    .unwrap();
    assert!(updated.changed);
    assert_eq!(updated.list.revision, 2);

    let replay_after_edit = create_list(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "My list",
        empty_filter(),
    )
    .await;
    assert!(!replay_after_edit.created);
    assert_eq!(replay_after_edit.list.name, "Renamed");
    assert_eq!(replay_after_edit.list.revision, 2);

    let no_op = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 2,
            name: "Renamed".to_string(),
            filter: empty_filter(),
        },
    )
    .await
    .unwrap();
    assert!(!no_op.changed);
    assert_eq!(no_op.list.revision, 2);

    // Commands can be constructed directly by another Rust caller. Reject a
    // future definition version before an update has a chance to touch the
    // row, just as Create does below.
    let before_bad_update: (String, i64, Value) =
        sqlx::query_as("SELECT name, revision, filter FROM saved_list WHERE id = $1")
            .bind(created.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    let invalid_update = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 2,
            name: "must not write".to_string(),
            filter: FilterDefinition {
                version: 2,
                clauses: Vec::new(),
            },
        },
    )
    .await;
    assert!(matches!(
        invalid_update,
        Err(SavedListError::MalformedRequest)
    ));
    let after_bad_update: (String, i64, Value) =
        sqlx::query_as("SELECT name, revision, filter FROM saved_list WHERE id = $1")
            .bind(created.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(
        before_bad_update, after_bad_update,
        "manual non-v1 update must not write"
    );

    let deleted = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: created.list.id,
            expected_revision: 2,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);
    let tombstone: (Option<String>, Option<String>, i64) =
        sqlx::query_as("SELECT name, filter::text, revision FROM saved_list WHERE id = $1")
            .bind(created.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(tombstone.0, None);
    assert_eq!(tombstone.1, None);
    assert_eq!(tombstone.2, 3);

    let repeat_delete = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: created.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    assert!(repeat_delete.deleted);
    let revision_after_repeat: i64 =
        sqlx::query_scalar("SELECT revision FROM saved_list WHERE id = $1")
            .bind(created.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(revision_after_repeat, 3, "tombstone retry is write-free");

    let deleted_replay = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        CreateSavedList {
            request_id,
            scope: SavedListScope::Personal,
            name: "My list".to_string(),
            filter: empty_filter(),
        },
    )
    .await;
    assert!(matches!(deleted_replay, Err(SavedListError::Deleted)));

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_list")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    let invalid_direct = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: "Bad direct version".to_string(),
            filter: FilterDefinition {
                version: 2,
                clauses: Vec::new(),
            },
        },
    )
    .await;
    assert!(matches!(
        invalid_direct,
        Err(SavedListError::MalformedRequest)
    ));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_list")
        .fetch_one(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(before, after, "manual non-v1 filter must not write");
}

#[sqlx::test]
#[ignore]
async fn create_retry_identity_is_scoped_to_actor_and_organization(migrator_pool: PgPool) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list retry scope A",
        "alice@saved-list-retry.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(&migrator_pool, "bob@saved-list-retry.test", "Bob", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let second_organization_id = create_org(&migrator_pool, "Saved list retry scope B").await;
    add_membership_with(
        &migrator_pool,
        second_organization_id,
        alice_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let request_id = Uuid::new_v4();

    let alice_first = create_list(
        &app_pool,
        organization_id,
        alice_id,
        request_id,
        SavedListScope::Personal,
        "Same request",
        empty_filter(),
    )
    .await;
    let bob_same_token = create_list(
        &app_pool,
        organization_id,
        bob_id,
        request_id,
        SavedListScope::Personal,
        "Same request",
        empty_filter(),
    )
    .await;
    let alice_other_org = create_list(
        &app_pool,
        second_organization_id,
        alice_id,
        request_id,
        SavedListScope::Personal,
        "Same request",
        empty_filter(),
    )
    .await;
    assert!(alice_first.created && bob_same_token.created && alice_other_org.created);
    assert_ne!(alice_first.list.id, bob_same_token.list.id);
    assert_ne!(alice_first.list.id, alice_other_org.list.id);

    let changed_payload = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, alice_id),
        CreateSavedList {
            request_id,
            scope: SavedListScope::Personal,
            name: "Changed payload".to_string(),
            filter: empty_filter(),
        },
    )
    .await;
    assert!(matches!(
        changed_payload,
        Err(SavedListError::RequestConflict)
    ));
}

#[sqlx::test]
#[ignore]
async fn visibility_and_current_membership_permissions_do_not_leak_personal_lists(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list privacy",
        "owner@saved-list-privacy.test",
        "Owner",
        PW,
    )
    .await;
    let admin_id = create_user(&migrator_pool, "admin@saved-list-privacy.test", "Admin", PW).await;
    let member_id = create_user(
        &migrator_pool,
        "member@saved-list-privacy.test",
        "Member",
        PW,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let personal = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Owner only",
        empty_filter(),
    )
    .await;
    let owner_auth = auth_context(organization_id, owner_id, Role::Member);
    let admin_auth = auth_context(organization_id, admin_id, Role::Admin);
    let member_auth = auth_context(organization_id, member_id, Role::Member);
    let mut conn = app_pool.acquire().await.unwrap();
    assert!(
        saved_list::saved_list_detail(&mut conn, &admin_auth, personal.list.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        saved_list::count_saved_list_matches(&mut conn, &member_auth, personal.list.id, 1)
            .await
            .unwrap()
            .is_none()
    );
    drop(conn);

    let hidden_update = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, admin_id),
        UpdateSavedList {
            list_id: personal.list.id,
            expected_revision: 1,
            name: "Attempted reveal".to_string(),
            filter: empty_filter(),
        },
    )
    .await;
    assert!(matches!(hidden_update, Err(SavedListError::NotFound)));

    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Team queue",
        empty_filter(),
    )
    .await;
    let mut conn = app_pool.acquire().await.unwrap();
    assert!(
        saved_list::saved_list_detail(&mut conn, &member_auth, shared.list.id)
            .await
            .unwrap()
            .is_some()
    );
    drop(conn);
    let member_update = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, member_id),
        UpdateSavedList {
            list_id: shared.list.id,
            expected_revision: 1,
            name: "Nope".to_string(),
            filter: empty_filter(),
        },
    )
    .await;
    assert!(matches!(member_update, Err(SavedListError::Forbidden)));

    saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, admin_id),
        DeleteSavedList {
            list_id: shared.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    let member_tombstone_delete = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, member_id),
        DeleteSavedList {
            list_id: shared.list.id,
            expected_revision: 1,
        },
    )
    .await;
    assert!(matches!(
        member_tombstone_delete,
        Err(SavedListError::NotFound)
    ));
    let admin_tombstone_delete = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, admin_id),
        DeleteSavedList {
            list_id: shared.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    assert!(admin_tombstone_delete.deleted);

    // Commands lock and inspect the current membership, rather than trusting
    // a session-derived/previous role. Deactivation preserves the personal
    // row while stopping a new mutation.
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(owner_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let inactive_mutation = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: personal.list.id,
            expected_revision: 1,
        },
    )
    .await;
    assert!(matches!(
        inactive_mutation,
        Err(SavedListError::Unauthenticated)
    ));
    let mut conn = app_pool.acquire().await.unwrap();
    assert!(
        saved_list::saved_list_detail(&mut conn, &owner_auth, personal.list.id)
            .await
            .unwrap()
            .is_some()
    );
}

// --- Quotas / capped current counts ---------------------------------------

#[sqlx::test]
#[ignore]
async fn saved_list_scope_quotas_are_separate_and_deletion_frees_a_slot(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list quotas",
        "owner@saved-list-quotas.test",
        "Owner",
        PW,
    )
    .await;
    let admin_id = create_user(&migrator_pool, "admin@saved-list-quotas.test", "Admin", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let other_owner_id = create_user(
        &migrator_pool,
        "other-owner@saved-list-quotas.test",
        "Other owner",
        PW,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        other_owner_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    insert_live_seed_rows(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        49,
    )
    .await;
    let fiftieth_request_id = Uuid::new_v4();
    let fiftieth = create_list(
        &app_pool,
        organization_id,
        owner_id,
        fiftieth_request_id,
        SavedListScope::Personal,
        "50th personal",
        empty_filter(),
    )
    .await;
    let over_personal = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: "51st personal".to_string(),
            filter: empty_filter(),
        },
    )
    .await;
    assert!(matches!(over_personal, Err(SavedListError::LimitReached)));
    let full_quota_replay = create_list(
        &app_pool,
        organization_id,
        owner_id,
        fiftieth_request_id,
        SavedListScope::Personal,
        "50th personal",
        empty_filter(),
    )
    .await;
    assert!(
        !full_quota_replay.created && full_quota_replay.list.id == fiftieth.list.id,
        "the original retry must still succeed at the owner's full quota"
    );

    insert_live_seed_rows(
        &app_pool,
        organization_id,
        admin_id,
        SavedListScope::Shared,
        199,
    )
    .await;
    let two_hundredth = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "200th shared",
        empty_filter(),
    )
    .await;
    let over_shared = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, admin_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Shared,
            name: "201st shared".to_string(),
            filter: empty_filter(),
        },
    )
    .await;
    assert!(matches!(over_shared, Err(SavedListError::LimitReached)));

    // There is deliberately no combined Organization cap: the Organization
    // now has 200 shared + this owner's 50 personal rows, and a different
    // owner's first personal row must still be admitted.
    let other_owner_personal = create_list(
        &app_pool,
        organization_id,
        other_owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Other owner at combined 250",
        empty_filter(),
    )
    .await;
    assert!(other_owner_personal.created);

    let deleted = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: fiftieth.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);
    let replacement = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "freed personal slot",
        empty_filter(),
    )
    .await;
    assert!(replacement.created);
    assert!(
        two_hundredth.created,
        "shared cap is independent of personal cap"
    );
}

#[sqlx::test]
#[ignore]
async fn concurrent_create_and_revision_races_preserve_one_winner(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list races",
        "owner@saved-list-races.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    // Two callers release together with the same logical create. The
    // transaction advisory lock + scoped retry identity must yield one row,
    // one created response, and one replay response.
    let request_id = Uuid::new_v4();
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            saved_list::create_saved_list(
                &pool,
                &command_context(organization_id, owner_id),
                CreateSavedList {
                    request_id,
                    scope: SavedListScope::Personal,
                    name: "Concurrent retry".to_string(),
                    filter: empty_filter(),
                },
            )
            .await
        }));
    }
    barrier.wait().await;
    let first = tasks.remove(0).await.unwrap().unwrap();
    let second = tasks.remove(0).await.unwrap().unwrap();
    assert_ne!(first.created, second.created);
    assert_eq!(first.list.id, second.list.id);
    let one_row: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM saved_list WHERE organization_id = $1 AND create_request_id = $2",
    )
    .bind(organization_id)
    .bind(request_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(one_row, 1);

    // Seed through 49 and synchronously race two distinct final-slot
    // creates. The advisory lock serializes the count+insert pair, so only
    // one can become the fiftieth live personal list.
    insert_live_seed_rows(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        48,
    )
    .await;
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for name in ["final slot A", "final slot B"] {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            saved_list::create_saved_list(
                &pool,
                &command_context(organization_id, owner_id),
                CreateSavedList {
                    request_id: Uuid::new_v4(),
                    scope: SavedListScope::Personal,
                    name: name.to_string(),
                    filter: empty_filter(),
                },
            )
            .await
        }));
    }
    barrier.wait().await;
    let final_a = tasks.remove(0).await.unwrap();
    let final_b = tasks.remove(0).await.unwrap();
    assert_eq!(
        [final_a.is_ok(), final_b.is_ok()]
            .into_iter()
            .filter(|ok| *ok)
            .count(),
        1
    );
    assert!(matches!(
        (&final_a, &final_b),
        (Err(SavedListError::LimitReached), Ok(_)) | (Ok(_), Err(SavedListError::LimitReached))
    ));
    let live_personal: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM saved_list WHERE organization_id = $1 AND created_by_user_id = $2 AND scope = 'personal' AND deleted_at IS NULL",
    )
    .bind(organization_id)
    .bind(owner_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(live_personal, 50);

    // Same-revision updates race on the same row. One succeeds; the other
    // observes the committed revision and must conflict rather than silently
    // overwrite.
    saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: first.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    let race_list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "revision race",
        empty_filter(),
    )
    .await;
    // The owner is at quota now, but updates do not consume a slot.
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for name in ["race one", "race two"] {
        let pool = app_pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            saved_list::update_saved_list(
                &pool,
                &command_context(organization_id, owner_id),
                UpdateSavedList {
                    list_id: race_list.list.id,
                    expected_revision: 1,
                    name: name.to_string(),
                    filter: empty_filter(),
                },
            )
            .await
        }));
    }
    barrier.wait().await;
    let update_a = tasks.remove(0).await.unwrap();
    let update_b = tasks.remove(0).await.unwrap();
    assert!(matches!(
        (&update_a, &update_b),
        (Ok(_), Err(SavedListError::Conflict)) | (Err(SavedListError::Conflict), Ok(_))
    ));

    // An update and delete released at the same revision never produce a
    // hidden last-writer-wins loss. If delete wins, update sees the normal
    // live-row 404; if update wins, delete sees its stale conflict.
    let barrier = Arc::new(Barrier::new(3));
    let update_pool = app_pool.clone();
    let update_barrier = barrier.clone();
    let update_task = tokio::spawn(async move {
        update_barrier.wait().await;
        saved_list::update_saved_list(
            &update_pool,
            &command_context(organization_id, owner_id),
            UpdateSavedList {
                list_id: race_list.list.id,
                expected_revision: 2,
                name: "update-delete winner".to_string(),
                filter: empty_filter(),
            },
        )
        .await
    });
    let delete_pool = app_pool.clone();
    let delete_barrier = barrier.clone();
    let delete_task = tokio::spawn(async move {
        delete_barrier.wait().await;
        saved_list::delete_saved_list(
            &delete_pool,
            &command_context(organization_id, owner_id),
            DeleteSavedList {
                list_id: race_list.list.id,
                expected_revision: 2,
            },
        )
        .await
    });
    barrier.wait().await;
    let update_result = update_task.await.unwrap();
    let delete_result = delete_task.await.unwrap();
    assert!(matches!(
        (&update_result, &delete_result),
        (Ok(_), Err(SavedListError::Conflict)) | (Err(SavedListError::NotFound), Ok(_))
    ));
}

#[sqlx::test]
#[ignore]
async fn membership_for_share_lock_rechecks_role_and_status_after_a_privilege_race(
    migrator_pool: PgPool,
) {
    let (organization_id, admin_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved membership race",
        "admin@saved-membership-race.test",
        "Admin",
        PW,
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let app_pool = connect_as_app(&migrator_pool).await;
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Race-managed shared",
        empty_filter(),
    )
    .await;

    // A real row lock deliberately holds the membership change while the
    // command is released. Its `FOR SHARE` cannot pass this transaction; on
    // commit it must see the new member role rather than a stale session role.
    let mut role_tx = migrator_pool.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM organization_membership WHERE organization_id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(organization_id)
    .bind(admin_id)
    .fetch_one(&mut *role_tx)
    .await
    .unwrap();
    let release = Arc::new(Barrier::new(2));
    let task_pool = app_pool.clone();
    let task_release = release.clone();
    let mut role_task = tokio::spawn(async move {
        task_release.wait().await;
        saved_list::update_saved_list(
            &task_pool,
            &command_context(organization_id, admin_id),
            UpdateSavedList {
                list_id: shared.list.id,
                expected_revision: 1,
                name: "must not write after demotion".to_string(),
                filter: empty_filter(),
            },
        )
        .await
    });
    release.wait().await;
    assert!(
        timeout(Duration::from_millis(75), &mut role_task)
            .await
            .is_err(),
        "the command must wait on the held membership lock"
    );
    sqlx::query(
        "UPDATE organization_membership SET role = 'member' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&mut *role_tx)
    .await
    .unwrap();
    role_tx.commit().await.unwrap();
    assert!(matches!(
        role_task.await.unwrap(),
        Err(SavedListError::Forbidden)
    ));

    // Restore admin, then repeat with status deactivation. This proves the
    // same transaction-local read gates all shared mutations, not only role.
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin', status = 'active' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let mut status_tx = migrator_pool.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM organization_membership WHERE organization_id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(organization_id)
    .bind(admin_id)
    .fetch_one(&mut *status_tx)
    .await
    .unwrap();
    let release = Arc::new(Barrier::new(2));
    let task_pool = app_pool.clone();
    let task_release = release.clone();
    let mut status_task = tokio::spawn(async move {
        task_release.wait().await;
        saved_list::delete_saved_list(
            &task_pool,
            &command_context(organization_id, admin_id),
            DeleteSavedList {
                list_id: shared.list.id,
                expected_revision: 1,
            },
        )
        .await
    });
    release.wait().await;
    assert!(
        timeout(Duration::from_millis(75), &mut status_task)
            .await
            .is_err(),
        "the command must wait on the held membership lock"
    );
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&mut *status_tx)
    .await
    .unwrap();
    status_tx.commit().await.unwrap();
    assert!(matches!(
        status_task.await.unwrap(),
        Err(SavedListError::Unauthenticated)
    ));
}

#[sqlx::test]
#[ignore]
async fn saved_count_uses_same_capped_semantics_and_resolves_me_per_viewer(migrator_pool: PgPool) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved count parity",
        "alice@saved-count-parity.test",
        "Alice",
        PW,
    )
    .await;
    let bob_id = create_user(&migrator_pool, "bob@saved-count-parity.test", "Bob", PW).await;
    let admin_id = create_user(&migrator_pool, "admin@saved-count-parity.test", "Admin", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&migrator_pool, organization_id).await;
    insert_people_batch(&app_pool, organization_id, stage_id, 501, Some(alice_id)).await;
    insert_people_batch(&app_pool, organization_id, stage_id, 2, Some(bob_id)).await;

    let all = create_list(
        &app_pool,
        organization_id,
        alice_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "All people",
        empty_filter(),
    )
    .await;
    let alice_auth = auth_context(organization_id, alice_id, Role::Member);
    let mut conn = app_pool.acquire().await.unwrap();
    let all_count = saved_list::count_saved_list_matches(&mut conn, &alice_auth, all.list.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(all_count.count, 500);
    assert!(all_count.truncated);
    let scope = PersonVisibilityScope::from_auth(&alice_auth);
    let params = empty_filter().to_query_params(UserId::new(alice_id));
    let (summaries, summaries_truncated) =
        person_queries::filtered_summaries(&mut conn, &scope, &params)
            .await
            .unwrap();
    assert_eq!(summaries.len(), 500);
    assert!(summaries_truncated);
    drop(conn);

    let me_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::Me],
        })],
    };
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "My assigned",
        me_filter,
    )
    .await;
    let bob_auth = auth_context(organization_id, bob_id, Role::Member);
    let mut conn = app_pool.acquire().await.unwrap();
    let alice_count =
        saved_list::count_saved_list_matches(&mut conn, &alice_auth, shared.list.id, 1)
            .await
            .unwrap()
            .unwrap();
    let bob_count = saved_list::count_saved_list_matches(&mut conn, &bob_auth, shared.list.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(alice_count.count, 500);
    assert!(alice_count.truncated);
    assert_eq!(bob_count.count, 2);
    assert!(!bob_count.truncated);
}

#[sqlx::test]
#[ignore]
async fn saved_count_matches_people_for_every_filter_axis_mixed_and_exact_caps(
    migrator_pool: PgPool,
) {
    let (organization_id, alice_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved count every axis",
        "alice@saved-count-every-axis.test",
        "Alice",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let stage_id = first_stage_id(&migrator_pool, organization_id).await;
    let matching_person =
        insert_axis_person(&app_pool, organization_id, stage_id, Some(alice_id)).await;
    let _never_person = insert_axis_person(&app_pool, organization_id, stage_id, None).await;
    insert_axis_facts(&app_pool, organization_id, matching_person, alice_id).await;

    // A matching phone in another Organization must not affect either the
    // saved-list count or the existing People summaries for this scope.
    let foreign_organization_id = create_org(&migrator_pool, "Saved count foreign match").await;
    crate::common::seed_stages(&migrator_pool, foreign_organization_id).await;
    let foreign_stage_id = first_stage_id(&migrator_pool, foreign_organization_id).await;
    let foreign_person =
        insert_axis_person(&app_pool, foreign_organization_id, foreign_stage_id, None).await;
    sqlx::query(
        "INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value) \
         VALUES ($1, $2, 'phone', '+15557654321', '+15557654321')",
    )
    .bind(foreign_organization_id)
    .bind(foreign_person)
    .execute(&app_pool)
    .await
    .unwrap();

    let cases = vec![
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::Stage(StageClause {
                    stage_ids: vec![crm_api::ids::StageId::new(stage_id)],
                })],
            },
            2,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Me],
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Unassigned],
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::AssignedTo(AssignedToClause {
                    assignees: vec![Assignee::Me, Assignee::Unassigned],
                })],
            },
            2,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::Source(SourceClause {
                    sources: vec!["zillow".to_string()],
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::Source(SourceClause {
                    sources: vec!["old_source".to_string()],
                })],
            },
            0,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::Created(AgeClause {
                    age: AgeSpec::WithinDays(1),
                })],
            },
            2,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::Created(AgeClause {
                    age: AgeSpec::NotWithinDays(1),
                })],
            },
            0,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastInquiry(AgeClause {
                    age: AgeSpec::WithinDays(1),
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastInquiry(AgeClause {
                    age: AgeSpec::NotWithinDays(1),
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastInquiry(AgeClause {
                    age: AgeSpec::Never,
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastContact(AgeClause {
                    age: AgeSpec::WithinDays(1),
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastContact(AgeClause {
                    age: AgeSpec::NotWithinDays(1),
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastInbound(AgeClause {
                    age: AgeSpec::WithinDays(1),
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastInbound(AgeClause {
                    age: AgeSpec::NotWithinDays(1),
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastInbound(AgeClause {
                    age: AgeSpec::Never,
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::HasReplied(BoolClause { value: true })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::HasReplied(BoolClause { value: false })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::HasPhone(BoolClause { value: true })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::HasPhone(BoolClause { value: false })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::HasEmail(BoolClause { value: true })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::HasEmail(BoolClause { value: false })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::LastContact(AgeClause {
                    age: AgeSpec::Never,
                })],
            },
            1,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![Clause::Source(SourceClause {
                    sources: vec!["realtor_com".to_string()],
                })],
            },
            0,
        ),
        (
            FilterDefinition {
                version: 1,
                clauses: vec![
                    Clause::Stage(StageClause {
                        stage_ids: vec![crm_api::ids::StageId::new(stage_id)],
                    }),
                    Clause::AssignedTo(AssignedToClause {
                        assignees: vec![Assignee::Me],
                    }),
                    Clause::Source(SourceClause {
                        sources: vec!["zillow".to_string()],
                    }),
                    Clause::Created(AgeClause {
                        age: AgeSpec::WithinDays(1),
                    }),
                    Clause::LastInquiry(AgeClause {
                        age: AgeSpec::WithinDays(1),
                    }),
                    Clause::LastContact(AgeClause {
                        age: AgeSpec::WithinDays(1),
                    }),
                    Clause::LastInbound(AgeClause {
                        age: AgeSpec::WithinDays(1),
                    }),
                    Clause::HasReplied(BoolClause { value: true }),
                    Clause::HasPhone(BoolClause { value: true }),
                    Clause::HasEmail(BoolClause { value: true }),
                ],
            },
            1,
        ),
    ];
    for (filter, expected_count) in cases {
        assert_saved_count_parity(&app_pool, organization_id, alice_id, filter, expected_count)
            .await;
    }

    // Exact cap boundaries are a separate fixture so the axis cases above
    // stay readable. 500 is exact; adding exactly one makes the same dynamic
    // definition report the honest 500+ state.
    let cap_organization_id = create_org(&migrator_pool, "Saved count cap boundary").await;
    crate::common::seed_stages(&migrator_pool, cap_organization_id).await;
    let cap_user_id =
        create_user(&migrator_pool, "cap@saved-count-every-axis.test", "Cap", PW).await;
    crate::common::add_membership(&migrator_pool, cap_organization_id, cap_user_id).await;
    let cap_stage_id = first_stage_id(&migrator_pool, cap_organization_id).await;
    insert_people_batch(
        &app_pool,
        cap_organization_id,
        cap_stage_id,
        500,
        Some(cap_user_id),
    )
    .await;
    let cap_list = create_list(
        &app_pool,
        cap_organization_id,
        cap_user_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Exactly five hundred",
        empty_filter(),
    )
    .await;
    let cap_auth = auth_context(cap_organization_id, cap_user_id, Role::Member);
    let mut conn = app_pool.acquire().await.unwrap();
    let exact = saved_list::count_saved_list_matches(&mut conn, &cap_auth, cap_list.list.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((exact.count, exact.truncated), (500, false));
    drop(conn);
    insert_people_batch(
        &app_pool,
        cap_organization_id,
        cap_stage_id,
        1,
        Some(cap_user_id),
    )
    .await;
    let mut conn = app_pool.acquire().await.unwrap();
    let over = saved_list::count_saved_list_matches(&mut conn, &cap_auth, cap_list.list.id, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((over.count, over.truncated), (500, true));
}

// --- Stored definition failure handling -----------------------------------

#[sqlx::test]
#[ignore]
async fn unsupported_deep_stored_jsonb_keeps_metadata_retry_and_delete_safe(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Stored saved list",
        "owner@stored-saved-list.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let request_id = Uuid::new_v4();
    let created = create_list(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "Stored invalid",
        empty_filter(),
    )
    .await;

    // PostgreSQL accepts this JSONB object, while serde_json::Value's depth
    // guard intentionally declines it. It also has a future-version marker;
    // either structural reason must fail closed as unsupported, never 503 or
    // all-People.
    let nested = format!("{}0{}", "[".repeat(160), "]".repeat(160));
    let raw = format!(r#"{{"version":2,"clauses":[],"future":{nested}}}"#);
    sqlx::query("UPDATE saved_list SET filter = CAST($1 AS text)::jsonb WHERE id = $2")
        .bind(raw)
        .bind(created.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();

    let auth = auth_context(organization_id, owner_id, Role::Member);
    let mut conn = app_pool.acquire().await.unwrap();
    let detail = saved_list::saved_list_detail(&mut conn, &auth, created.list.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.list.name, "Stored invalid");
    assert!(detail.filter.is_none());
    assert!(matches!(
        detail.filter_error,
        Some(saved_list::SavedListFilterError::UnsupportedFilter)
    ));
    let count = saved_list::count_saved_list_matches(&mut conn, &auth, created.list.id, 1).await;
    assert!(matches!(count, Err(SavedListError::UnsupportedFilter)));
    drop(conn);

    let replay = create_list(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "Stored invalid",
        empty_filter(),
    )
    .await;
    assert!(!replay.created);
    assert_eq!(replay.list.id, created.list.id);
    saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: created.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
}

#[sqlx::test]
#[ignore]
async fn stored_v1_unknown_fields_and_bad_shapes_are_independently_unsupported(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Stored saved list shapes",
        "owner@stored-saved-list-shapes.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let auth = auth_context(organization_id, owner_id, Role::Member);

    for (name, raw) in [
        (
            "V1 unknown field",
            r#"{"version":1,"clauses":[],"future_field":true}"#,
        ),
        (
            "V1 malformed clauses",
            r#"{"version":1,"clauses":{"not":"an array"}}"#,
        ),
    ] {
        let created = create_list(
            &app_pool,
            organization_id,
            owner_id,
            Uuid::new_v4(),
            SavedListScope::Personal,
            name,
            empty_filter(),
        )
        .await;
        sqlx::query("UPDATE saved_list SET filter = CAST($1 AS text)::jsonb WHERE id = $2")
            .bind(raw)
            .bind(created.list.id.as_uuid())
            .execute(&migrator_pool)
            .await
            .unwrap();

        let mut conn = app_pool.acquire().await.unwrap();
        let detail = saved_list::saved_list_detail(&mut conn, &auth, created.list.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.list.name, name);
        assert!(detail.filter.is_none(), "{name}");
        assert!(matches!(
            detail.filter_error,
            Some(saved_list::SavedListFilterError::UnsupportedFilter)
        ));
        assert!(matches!(
            saved_list::count_saved_list_matches(&mut conn, &auth, created.list.id, 1).await,
            Err(SavedListError::UnsupportedFilter)
        ));
    }
}

#[sqlx::test]
#[ignore]
async fn stored_unknown_and_stale_filters_fail_closed_but_typed_stale_rows_can_repair(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved stored stale A",
        "owner@saved-stored-stale.test",
        "Owner",
        PW,
    )
    .await;
    let (other_organization_id, other_owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved stored stale B",
        "other@saved-stored-stale.test",
        "Other",
        PW,
    )
    .await;
    let inactive_id = create_user(
        &migrator_pool,
        "inactive@saved-stored-stale.test",
        "Inactive",
        PW,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        inactive_id,
        Role::Member,
        MembershipStatus::Inactive,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let other_stage_id = first_stage_id(&migrator_pool, other_organization_id).await;
    let auth = auth_context(organization_id, owner_id, Role::Member);

    let stale = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Stale stage",
        empty_filter(),
    )
    .await;
    let stale_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::Stage(StageClause {
            stage_ids: vec![crm_api::ids::StageId::new(other_stage_id)],
        })],
    };
    sqlx::query("UPDATE saved_list SET filter = CAST($1 AS text)::jsonb WHERE id = $2")
        .bind(serde_json::to_string(&stale_filter).unwrap())
        .bind(stale.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let stale_detail = saved_list::saved_list_detail(&mut conn, &auth, stale.list.id)
        .await
        .unwrap()
        .unwrap();
    assert!(stale_detail.filter.is_some());
    assert!(matches!(
        stale_detail.filter_error,
        Some(saved_list::SavedListFilterError::InvalidStage)
    ));
    assert!(matches!(
        saved_list::count_saved_list_matches(&mut conn, &auth, stale.list.id, 1).await,
        Err(SavedListError::InvalidStage)
    ));
    drop(conn);
    let repaired = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: stale.list.id,
            expected_revision: 1,
            name: "Repaired stage".to_string(),
            filter: empty_filter(),
        },
    )
    .await
    .unwrap();
    assert!(repaired.changed);

    let cross_org_assignee = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Cross organization assignee",
        empty_filter(),
    )
    .await;
    let cross_org_assignee_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::User(UserId::new(other_owner_id))],
        })],
    };
    sqlx::query("UPDATE saved_list SET filter = CAST($1 AS text)::jsonb WHERE id = $2")
        .bind(serde_json::to_string(&cross_org_assignee_filter).unwrap())
        .bind(cross_org_assignee.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let cross_org_detail =
        saved_list::saved_list_detail(&mut conn, &auth, cross_org_assignee.list.id)
            .await
            .unwrap()
            .unwrap();
    assert!(cross_org_detail.filter.is_some());
    assert!(matches!(
        cross_org_detail.filter_error,
        Some(saved_list::SavedListFilterError::InvalidAssignee)
    ));
    assert!(matches!(
        saved_list::count_saved_list_matches(&mut conn, &auth, cross_org_assignee.list.id, 1).await,
        Err(SavedListError::InvalidAssignee)
    ));
    drop(conn);

    let unknown = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Unknown future kind",
        empty_filter(),
    )
    .await;
    sqlx::query("UPDATE saved_list SET filter = '{\"version\":1,\"clauses\":[{\"kind\":\"future\",\"value\":true}]}'::jsonb WHERE id = $1")
        .bind(unknown.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();
    let mut conn = app_pool.acquire().await.unwrap();
    let unknown_detail = saved_list::saved_list_detail(&mut conn, &auth, unknown.list.id)
        .await
        .unwrap()
        .unwrap();
    assert!(unknown_detail.filter.is_none());
    assert!(matches!(
        unknown_detail.filter_error,
        Some(saved_list::SavedListFilterError::UnsupportedFilter)
    ));
    drop(conn);

    let inactive_filter = FilterDefinition {
        version: 1,
        clauses: vec![Clause::AssignedTo(AssignedToClause {
            assignees: vec![Assignee::User(UserId::new(inactive_id))],
        })],
    };
    let inactive = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Inactive assignee remains valid",
        inactive_filter,
    )
    .await;
    let mut conn = app_pool.acquire().await.unwrap();
    let inactive_detail = saved_list::saved_list_detail(&mut conn, &auth, inactive.list.id)
        .await
        .unwrap()
        .unwrap();
    assert!(inactive_detail.filter.is_some());
    assert!(inactive_detail.filter_error.is_none());

    // Keep the second fixture user live in the test's intended other-org
    // role; mentioning it makes the foreign-stage setup explicit and avoids
    // mistaking a missing global user for an Organization boundary test.
    assert_ne!(other_owner_id, owner_id);
}

// --- HTTP wire/error handling ---------------------------------------------

#[sqlx::test]
#[ignore]
async fn saved_list_http_is_strict_and_preserves_error_precedence(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list HTTP",
        "owner@saved-list-http.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let publisher = Publisher::recording();
    let router = build_router_with_publisher(&migrator_pool, publisher.clone()).await;
    let owner_cookie = login_cookie(&router, "owner@saved-list-http.test", PW).await;
    let request_id = Uuid::new_v4();
    let create_body = json!({
        "request_id": request_id,
        "scope": "personal",
        "name": "HTTP list",
        "filter": {"version": 1, "clauses": []},
    });
    let response =
        post_json_with_cookie(&router, "/api/saved-lists", &owner_cookie, create_body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = body_json(response).await;
    let id = created["list"]["id"].as_str().unwrap().to_string();

    // `revision` only accepts a single positive, canonical decimal token;
    // unrelated keys remain ignored like existing GET routes.
    for suffix in [
        "",
        "?revision=0",
        "?revision=01",
        "?revision=%2B1",
        "?revision=9999999999999999",
        "?revision=1&revision=1",
    ] {
        let response = get_with_cookie(
            &router,
            &format!("/api/saved-lists/{id}/count{suffix}"),
            &owner_cookie,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{suffix:?}");
    }
    let response = get_with_cookie(
        &router,
        &format!("/api/saved-lists/{id}/count?revision=1&unrelated=value"),
        &owner_cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let uppercase_request = request_id.to_string().to_uppercase();
    let response = post_raw(
        &router,
        "/api/saved-lists",
        &owner_cookie,
        format!(
            r#"{{"request_id":"{uppercase_request}","scope":"personal","name":"bad","filter":{{"version":1,"clauses":[]}}}}"#
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let duplicate_outer = format!(
        r#"{{"request_id":"{}","request_id":"{}","scope":"personal","name":"bad","filter":{{"version":1,"clauses":[]}}}}"#,
        Uuid::new_v4(),
        Uuid::new_v4()
    );
    let response = post_raw(&router, "/api/saved-lists", &owner_cookie, duplicate_outer).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let duplicate_nested = format!(
        r#"{{"request_id":"{}","scope":"personal","name":"bad","filter":{{"version":1,"clauses":[{{"kind":"has_phone","value":true,"value":false}}]}}}}"#,
        Uuid::new_v4()
    );
    let response = post_raw(&router, "/api/saved-lists", &owner_cookie, duplicate_nested).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let unknown_outer = format!(
        r#"{{"request_id":"{}","scope":"personal","name":"bad","filter":{{"version":1,"clauses":[]}},"unexpected":true}}"#,
        Uuid::new_v4(),
    );
    let response = post_raw(&router, "/api/saved-lists", &owner_cookie, unknown_outer).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let unknown_nested = format!(
        r#"{{"request_id":"{}","scope":"personal","name":"bad","filter":{{"version":1,"clauses":[{{"kind":"has_phone","value":true,"unexpected":true}}]}}}}"#,
        Uuid::new_v4(),
    );
    let response = post_raw(&router, "/api/saved-lists", &owner_cookie, unknown_nested).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Valid JSON plus trailing JSON whitespace proves the route's 128 KiB
    // extractor cap itself. An unknown padding field would only prove strict
    // envelope decoding, even if this cap disappeared.
    let oversized = |body: Value| format!("{}{}", body, " ".repeat(128 * 1024));
    for (method, uri, body) in [
        (
            "POST",
            "/api/saved-lists".to_string(),
            json!({
                "request_id": Uuid::new_v4(),
                "scope": "personal",
                "name": "valid but too large",
                "filter": {"version": 1, "clauses": []},
            }),
        ),
        (
            "PUT",
            format!("/api/saved-lists/{id}"),
            json!({
                "expected_revision": 1,
                "name": "valid but too large",
                "filter": {"version": 1, "clauses": []},
            }),
        ),
        (
            "DELETE",
            format!("/api/saved-lists/{id}"),
            json!({"expected_revision": 1}),
        ),
    ] {
        let response = raw_request(&router, method, &uri, &owner_cookie, oversized(body)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method}");
        assert_eq!(
            body_json(response).await,
            json!({"error":"malformed_request"}),
            "{method} must fail before a valid mutation is accepted"
        );
    }

    // A stale revision wins before reference validation, retaining the
    // explicit conflict contract for a stale form carrying a now-bad ID.
    let unknown_stage = Uuid::new_v4();
    let response = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{id}"),
        &owner_cookie,
        json!({
            "expected_revision": 2,
            "name": "stale",
            "filter": {"version": 1, "clauses": [{"kind":"stage","stage_ids":[unknown_stage]}]},
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(response).await,
        json!({"error":"saved_list_conflict"})
    );

    let response = delete_json(
        &router,
        &format!("/api/saved-lists/{id}"),
        &owner_cookie,
        json!({"expected_revision": 1}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, json!({"deleted": true}));

    // Ensure this test's direct app pool was only used as a production-role
    // fixture path, not a hidden migrator shortcut.
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM saved_list WHERE organization_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(organization_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    assert_eq!(rows, 1);

    // The request is authenticated first, then a required definition-label
    // lookup loses its DB privilege. Detail/count must surface infrastructure
    // failure as 503 rather than pretending the definition is invalid.
    let stage_id = first_stage_id(&migrator_pool, organization_id).await;
    let stage_definition = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Stage labels fail closed",
        FilterDefinition {
            version: 1,
            clauses: vec![Clause::Stage(StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(stage_id)],
            })],
        },
    )
    .await;
    sqlx::query("REVOKE SELECT ON TABLE stage FROM crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
    for uri in [
        format!("/api/saved-lists/{}", stage_definition.list.id),
        format!(
            "/api/saved-lists/{}/count?revision=1",
            stage_definition.list.id
        ),
    ] {
        let response = get_with_cookie(&router, &uri, &owner_cookie).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(body_json(response).await, json!({"error":"unavailable"}));
    }

    // Saved definition CRUD is private configuration only. It must not emit
    // person/list publications that could leak names, criteria, or retry
    // state into an Organization realtime channel.
    assert!(
        recorded(&publisher).await.is_empty(),
        "saved-list routes publish nothing"
    );
}

#[sqlx::test]
#[ignore]
async fn saved_list_http_index_is_metadata_only_and_actor_visible(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved HTTP index A",
        "owner@saved-http-index.test",
        "Owner",
        PW,
    )
    .await;
    let member_id = create_user(&migrator_pool, "member@saved-http-index.test", "Member", PW).await;
    let admin_id = create_user(&migrator_pool, "admin@saved-http-index.test", "Admin", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let (foreign_organization_id, foreign_owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved HTTP index B",
        "foreign@saved-http-index.test",
        "Foreign",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let _owner_personal = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Owner private index",
        empty_filter(),
    )
    .await;
    let _member_personal = create_list(
        &app_pool,
        organization_id,
        member_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Member private index",
        empty_filter(),
    )
    .await;
    let _admin_personal = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Admin private index",
        empty_filter(),
    )
    .await;
    let _shared_valid = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Shared valid index",
        empty_filter(),
    )
    .await;
    let shared_invalid = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Shared invalid index",
        empty_filter(),
    )
    .await;
    let _foreign_personal = create_list(
        &app_pool,
        foreign_organization_id,
        foreign_owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Foreign private index",
        empty_filter(),
    )
    .await;

    // The index must neither deserialize nor reject one malformed stored
    // definition. PostgreSQL accepts it, while serde_json declines this depth;
    // the shared row must still be visible as metadata to each actor.
    let nested = format!("{}0{}", "[".repeat(160), "]".repeat(160));
    let invalid_filter =
        format!(r#"{{"version":2,"clauses":[],"SENTINEL_INDEX_FILTER_DO_NOT_EXPOSE":{nested}}}"#);
    sqlx::query("UPDATE saved_list SET filter = CAST($1 AS text)::jsonb WHERE id = $2")
        .bind(invalid_filter)
        .bind(shared_invalid.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();

    let router = build_router(&migrator_pool).await;
    let member_cookie = login_cookie(&router, "member@saved-http-index.test", PW).await;
    let admin_cookie = login_cookie(&router, "admin@saved-http-index.test", PW).await;

    for (actor, cookie, expected_names) in [
        (
            "member",
            &member_cookie,
            vec![
                "Member private index",
                "Shared valid index",
                "Shared invalid index",
            ],
        ),
        (
            "admin",
            &admin_cookie,
            vec![
                "Admin private index",
                "Shared valid index",
                "Shared invalid index",
            ],
        ),
    ] {
        let response = get_with_cookie(&router, "/api/saved-lists", cookie).await;
        assert_eq!(response.status(), StatusCode::OK, "{actor}");
        let body = body_json(response).await;
        assert!(
            !body
                .to_string()
                .contains("SENTINEL_INDEX_FILTER_DO_NOT_EXPOSE"),
            "{actor} index must not expose stored filter content"
        );
        let lists = body["lists"].as_array().expect("lists array");
        let mut names = lists
            .iter()
            .map(|list| list["name"].as_str().expect("metadata name").to_string())
            .collect::<Vec<_>>();
        names.sort();
        let mut expected_names = expected_names
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        expected_names.sort();
        assert_eq!(names, expected_names, "{actor} visibility");

        for list in lists {
            let object = list.as_object().expect("metadata object");
            let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
            keys.sort_unstable();
            assert_eq!(
                keys,
                vec![
                    "can_delete",
                    "can_edit",
                    "created_at",
                    "id",
                    "name",
                    "revision",
                    "scope",
                    "updated_at",
                ],
                "{actor} index is metadata-only"
            );
            let is_own_personal = actor == "member" && list["name"] == "Member private index";
            assert_eq!(list["can_edit"], json!(actor == "admin" || is_own_personal));
            assert_eq!(
                list["can_delete"],
                json!(actor == "admin" || is_own_personal)
            );
        }

        let invalid = lists
            .iter()
            .find(|list| list["id"] == json!(shared_invalid.list.id))
            .expect("invalid stored shared row remains indexed");
        assert_eq!(invalid["scope"], "shared");
        assert!(invalid.get("filter").is_none());
    }
}

#[sqlx::test]
#[ignore]
async fn saved_list_http_hides_foreign_and_private_ids_with_the_same_404(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved HTTP privacy A",
        "owner@saved-http-privacy.test",
        "Owner",
        PW,
    )
    .await;
    let member_id = create_user(
        &migrator_pool,
        "member@saved-http-privacy.test",
        "Member",
        PW,
    )
    .await;
    let admin_id = create_user(&migrator_pool, "admin@saved-http-privacy.test", "Admin", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let (foreign_organization_id, foreign_owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved HTTP privacy B",
        "foreign@saved-http-privacy.test",
        "Foreign",
        PW,
    )
    .await;
    let platform_id = create_platform_admin(
        &migrator_pool,
        "platform@saved-http-privacy.test",
        "Platform",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let personal = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Owner private",
        empty_filter(),
    )
    .await;
    let foreign = create_list(
        &app_pool,
        foreign_organization_id,
        foreign_owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Foreign private",
        empty_filter(),
    )
    .await;
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Shared readable",
        empty_filter(),
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let member_cookie = login_cookie(&router, "member@saved-http-privacy.test", PW).await;
    let admin_cookie = login_cookie(&router, "admin@saved-http-privacy.test", PW).await;
    let platform_cookie = login_cookie(&router, "platform@saved-http-privacy.test", PW).await;
    assert_ne!(platform_id, owner_id);

    const NOT_FOUND: &[u8] = br#"{"error":"not_found"}"#;
    const FORBIDDEN: &[u8] = br#"{"error":"forbidden"}"#;
    const DELETED: &[u8] = br#"{"deleted":true}"#;

    // Every hidden-ID route uses the same envelope bytes. Exercise both
    // member and admin callers: an admin has shared-list authority but never
    // visibility into another actor's personal definitions.
    let hidden_lists = [
        ("other-owner personal", personal.list.id.as_uuid()),
        ("foreign personal", foreign.list.id.as_uuid()),
        ("random", Uuid::new_v4()),
    ];
    for (actor, cookie) in [
        ("member", member_cookie.as_str()),
        ("admin", admin_cookie.as_str()),
    ] {
        for &(hidden_kind, list_id) in &hidden_lists {
            let detail_uri = format!("/api/saved-lists/{list_id}");
            let count_uri = format!("{detail_uri}/count?revision=1");

            let label = format!("{actor} {hidden_kind} detail");
            assert_status_body(
                get_with_cookie(&router, &detail_uri, cookie).await,
                StatusCode::NOT_FOUND,
                NOT_FOUND,
                &label,
            )
            .await;

            let label = format!("{actor} {hidden_kind} count");
            assert_status_body(
                get_with_cookie(&router, &count_uri, cookie).await,
                StatusCode::NOT_FOUND,
                NOT_FOUND,
                &label,
            )
            .await;

            let label = format!("{actor} {hidden_kind} update");
            assert_status_body(
                put_json_with_cookie(
                    &router,
                    &detail_uri,
                    cookie,
                    json!({
                        "expected_revision": 1,
                        "name": "Hidden mutation attempt",
                        "filter": {"version": 1, "clauses": []},
                    }),
                )
                .await,
                StatusCode::NOT_FOUND,
                NOT_FOUND,
                &label,
            )
            .await;

            let label = format!("{actor} {hidden_kind} delete");
            assert_status_body(
                delete_json(
                    &router,
                    &detail_uri,
                    cookie,
                    json!({"expected_revision": 1}),
                )
                .await,
                StatusCode::NOT_FOUND,
                NOT_FOUND,
                &label,
            )
            .await;
        }
    }

    let shared_uri = format!("/api/saved-lists/{}", shared.list.id);
    let shared_read = get_with_cookie(&router, &shared_uri, &member_cookie).await;
    assert_eq!(
        shared_read.status(),
        StatusCode::OK,
        "member reads live shared"
    );
    assert_status_body(
        put_json_with_cookie(
            &router,
            &shared_uri,
            &member_cookie,
            json!({
                "expected_revision": 1,
                "name": "Shared mutation attempt",
                "filter": {"version": 1, "clauses": []},
            }),
        )
        .await,
        StatusCode::FORBIDDEN,
        FORBIDDEN,
        "member cannot update live shared",
    )
    .await;

    assert_status_body(
        delete_json(
            &router,
            &shared_uri,
            &admin_cookie,
            json!({"expected_revision": 1}),
        )
        .await,
        StatusCode::OK,
        DELETED,
        "admin deletes shared",
    )
    .await;
    assert_status_body(
        delete_json(
            &router,
            &shared_uri,
            &member_cookie,
            json!({"expected_revision": 1}),
        )
        .await,
        StatusCode::NOT_FOUND,
        NOT_FOUND,
        "member cannot probe shared tombstone",
    )
    .await;
    assert_status_body(
        delete_json(
            &router,
            &shared_uri,
            &admin_cookie,
            json!({"expected_revision": 1}),
        )
        .await,
        StatusCode::OK,
        DELETED,
        "admin retry succeeds for shared tombstone",
    )
    .await;

    let platform_read = get_with_cookie(&router, "/api/saved-lists", &platform_cookie).await;
    assert_status_body(
        platform_read,
        StatusCode::UNAUTHORIZED,
        br#"{"error":"unauthenticated"}"#,
        "platform user has no active organization",
    )
    .await;
}

#[sqlx::test]
#[ignore]
async fn saved_list_spans_record_static_kinds_without_names_filters_or_retry_tokens(
    migrator_pool: PgPool,
) {
    use std::sync::Mutex;

    use tracing_subscriber::layer::SubscriberExt;

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CaptureWriter {
        type Writer = CaptureWriter;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved telemetry",
        "owner@saved-telemetry.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-telemetry.test", PW).await;
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    let guard = tracing::subscriber::set_default(subscriber);
    let request_id = Uuid::new_v4();
    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "SENTINEL_LIST_NAME_DO_NOT_LOG",
        FilterDefinition {
            version: 1,
            clauses: vec![Clause::Source(SourceClause {
                sources: vec!["sentinel_source_do_not_log".to_string()],
            })],
        },
    )
    .await;
    let updated = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: list.list.id,
            expected_revision: 1,
            name: "SENTINEL_UPDATED_LIST_NAME_DO_NOT_LOG".to_string(),
            filter: FilterDefinition {
                version: 1,
                clauses: vec![Clause::Source(SourceClause {
                    sources: vec!["sentinel_source_do_not_log".to_string()],
                })],
            },
        },
    )
    .await
    .unwrap();
    assert!(updated.changed);

    let response = get_with_cookie(
        &router,
        &format!("/api/saved-lists/{}/count?revision=2", list.list.id),
        &cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    drop(guard);

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(captured.contains("saved_list.create"));
    assert!(captured.contains("/api/saved-lists/"));
    assert!(
        captured.contains("filter_kinds=\"source\"") || captured.contains("filter_kinds=source"),
        "count span must record only the static filter kind: {captured}"
    );
    assert!(
        captured.contains("filter_clause_count=1"),
        "count span must record the number of clauses: {captured}"
    );
    assert!(
        captured.lines().any(|line| {
            line.contains("saved_list.update")
                && (line.contains("scope=\"personal\"") || line.contains("scope=personal"))
        }),
        "update span must record the resolved safe scope: {captured}"
    );
    let request_id_text = request_id.to_string();
    for secret in [
        "SENTINEL_LIST_NAME_DO_NOT_LOG",
        "SENTINEL_UPDATED_LIST_NAME_DO_NOT_LOG",
        "sentinel_source_do_not_log",
        request_id_text.as_str(),
        "revision=2",
    ] {
        assert!(!captured.contains(secret), "span/log leaked {secret:?}");
    }
}

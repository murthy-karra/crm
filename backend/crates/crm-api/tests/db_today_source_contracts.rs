//! Contract and persistence-edge coverage for Slice 011c Today sources.
//!
//! These tests deliberately use the public HTTP routes and typed command
//! layer. Direct SQL is limited to rollback-era and foreign-key fixtures that
//! ordinary callers cannot create.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::common::{
    add_membership_with, body_json, build_router, connect_as_app,
    create_org_with_stages_and_member, create_user, get_with_cookie, login_cookie,
    put_json_with_cookie,
};
use crm_api::auth::AuthContext;
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::FilterDefinition;
use crm_api::domain::saved_list::{
    self, CreateSavedList, SavedListError, SavedListScope, MAX_WIRE_REVISION,
};
use crm_api::domain::today::{self, EnableTodayWorkSource, TodaySource};
use crm_api::ids::{CorrelationId, OrganizationId, SavedListId, UserId};

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

async fn create_list(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    scope: SavedListScope,
    name: &str,
) -> saved_list::CreateSavedListOutcome {
    saved_list::create_saved_list(
        app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope,
            name: name.to_string(),
            filter: empty_filter(),
            sort: None,
        },
    )
    .await
    .unwrap()
}

async fn enable(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    list_id: SavedListId,
    expected_list_revision: i64,
) -> Result<today::TodaySourceChange, SavedListError> {
    today::enable_today_work_source(
        app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id,
            expected_list_revision,
        },
    )
    .await
}

async fn list_sources(
    app_pool: &PgPool,
    organization_id: Uuid,
    actor_user_id: Uuid,
    role: Role,
) -> Vec<TodaySource> {
    let mut conn = app_pool.acquire().await.unwrap();
    today::list_today_work_sources(
        &mut conn,
        &auth_context(organization_id, actor_user_id, role),
    )
    .await
    .unwrap()
}

async fn raw_request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: String,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    router
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

async fn response_bytes(response: Response, expected_status: StatusCode) -> Vec<u8> {
    assert_eq!(response.status(), expected_status);
    response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

/// The source route follows saved-list wire discipline: strict serde fields,
/// exact positive-safe-integer revisions, and a bounded body must all fail
/// before the source command can observe the list.
#[sqlx::test]
#[ignore]
async fn today_source_enable_http_rejects_noncanonical_or_non_strict_bodies_before_mutation(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source strict request",
        "owner@today-source-strict-request.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Strict wire queue",
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let owner_cookie = login_cookie(&router, "owner@today-source-strict-request.test", PW).await;
    let uri = format!("/api/today/sources/{}", list.list.id);
    let revision_above_max = format!(r#"{{"expected_list_revision":{}}}"#, MAX_WIRE_REVISION + 1);

    for body in [
        r#"{"expected_list_revision":1,"unexpected":true}"#,
        r#"{"expected_list_revision":1,"expected_list_revision":1}"#,
        r#"{"expected_list_revision":0}"#,
        r#"{"expected_list_revision":-1}"#,
        r#"{"expected_list_revision":1.0}"#,
        revision_above_max.as_str(),
    ] {
        let response =
            raw_request(&router, "PUT", &uri, Some(&owner_cookie), body.to_string()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "body {body}");
        assert_eq!(
            body_json(response).await,
            json!({ "error": "malformed_request" })
        );
    }

    let enabled = put_json_with_cookie(
        &router,
        &uri,
        &owner_cookie,
        json!({ "expected_list_revision": list.list.revision }),
    )
    .await;
    assert_eq!(enabled.status(), StatusCode::OK);
    assert_eq!(
        body_json(enabled).await,
        json!({ "enabled": true, "changed": true })
    );

    let over_limit = format!(
        r#"{{"expected_list_revision":{}}}{}"#,
        list.list.revision,
        " ".repeat(128 * 1024)
    );
    let response = raw_request(&router, "PUT", &uri, Some(&owner_cookie), over_limit).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await,
        json!({ "error": "malformed_request" })
    );

    assert_eq!(
        list_sources(&app_pool, organization_id, owner_id, Role::Member)
            .await
            .len(),
        1,
        "the rejected payloads must not alter the one canonical enable"
    );
}

/// A malformed UUID is rejected by the path extractor before session lookup;
/// syntactically valid paths still require a session for both mutations.
#[sqlx::test]
#[ignore]
async fn today_source_path_precedence_is_malformed_before_authentication(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source path precedence",
        "owner@today-source-path-precedence.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Path precedence queue",
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let valid_uri = format!("/api/today/sources/{}", list.list.id);

    for (method, uri, body, expected_status, expected_body) in [
        (
            "PUT",
            valid_uri.as_str(),
            r#"{"expected_list_revision":1}"#,
            StatusCode::UNAUTHORIZED,
            json!({ "error": "unauthenticated" }),
        ),
        (
            "DELETE",
            valid_uri.as_str(),
            "",
            StatusCode::UNAUTHORIZED,
            json!({ "error": "unauthenticated" }),
        ),
        (
            "PUT",
            "/api/today/sources/not-a-uuid",
            "not json",
            StatusCode::BAD_REQUEST,
            json!({ "error": "malformed_request" }),
        ),
        (
            "DELETE",
            "/api/today/sources/not-a-uuid",
            "",
            StatusCode::BAD_REQUEST,
            json!({ "error": "malformed_request" }),
        ),
    ] {
        let response = raw_request(&router, method, uri, None, body.to_string()).await;
        assert_eq!(response.status(), expected_status, "{method} {uri}");
        assert_eq!(body_json(response).await, expected_body, "{method} {uri}");
    }
}

/// Both typed callers and HTTP conceal private, foreign, and absent list IDs
/// from an ordinary member and an Organization admin alike.
#[sqlx::test]
#[ignore]
async fn today_source_private_foreign_and_missing_ids_are_indistinguishable_to_members_and_admins(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source private contract",
        "owner@today-source-private-contract.test",
        "Owner",
        PW,
    )
    .await;
    let member_id = create_user(
        &migrator_pool,
        "member@today-source-private-contract.test",
        "Member",
        PW,
    )
    .await;
    let admin_id = create_user(
        &migrator_pool,
        "admin@today-source-private-contract.test",
        "Admin",
        PW,
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
        "Foreign Today source private contract",
        "owner@foreign-today-source-private-contract.test",
        "Foreign owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let private = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Owner-only queue",
    )
    .await;
    let foreign = create_list(
        &app_pool,
        foreign_organization_id,
        foreign_owner_id,
        SavedListScope::Personal,
        "Foreign queue",
    )
    .await;
    let missing = SavedListId::new(Uuid::new_v4());

    for actor_user_id in [member_id, admin_id] {
        for (list_id, revision) in [
            (private.list.id, private.list.revision),
            (foreign.list.id, foreign.list.revision),
            (missing, 1),
        ] {
            assert!(matches!(
                enable(&app_pool, organization_id, actor_user_id, list_id, revision).await,
                Err(SavedListError::NotFound)
            ));
        }
    }
    for (list_id, revision) in [(foreign.list.id, foreign.list.revision), (missing, 1)] {
        assert!(matches!(
            enable(&app_pool, organization_id, owner_id, list_id, revision).await,
            Err(SavedListError::NotFound)
        ));
    }

    let router = build_router(&migrator_pool).await;
    for (email, actor_id) in [
        ("member@today-source-private-contract.test", member_id),
        ("admin@today-source-private-contract.test", admin_id),
        ("owner@today-source-private-contract.test", owner_id),
    ] {
        let cookie = login_cookie(&router, email, PW).await;
        let missing_response = put_json_with_cookie(
            &router,
            &format!("/api/today/sources/{missing}"),
            &cookie,
            json!({ "expected_list_revision": 1 }),
        )
        .await;
        let missing_bytes = response_bytes(missing_response, StatusCode::NOT_FOUND).await;
        let targets: &[(SavedListId, i64)] = if actor_id == owner_id {
            &[(foreign.list.id, foreign.list.revision)]
        } else {
            &[
                (private.list.id, private.list.revision),
                (foreign.list.id, foreign.list.revision),
            ]
        };
        for (list_id, revision) in targets {
            let response = put_json_with_cookie(
                &router,
                &format!("/api/today/sources/{list_id}"),
                &cookie,
                json!({ "expected_list_revision": revision }),
            )
            .await;
            assert_eq!(
                response_bytes(response, StatusCode::NOT_FOUND).await,
                missing_bytes,
                "{email} must receive byte-identical not-found responses"
            );
        }
    }
}

/// Preferences are retained while a member is inactive, but their current
/// membership is still rechecked in each mutation; reactivation restores the
/// old configuration without a backfill.
#[sqlx::test]
#[ignore]
async fn today_source_membership_deactivation_retains_preferences_and_reactivation_restores_them(
    migrator_pool: PgPool,
) {
    let (organization_id, member_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source membership lifecycle",
        "member@today-source-membership-lifecycle.test",
        "Member",
        PW,
    )
    .await;
    let admin_id = create_user(
        &migrator_pool,
        "admin@today-source-membership-lifecycle.test",
        "Admin",
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
    let app_pool = connect_as_app(&migrator_pool).await;
    let retained = create_list(
        &app_pool,
        organization_id,
        member_id,
        SavedListScope::Personal,
        "Retained queue",
    )
    .await;
    let blocked = create_list(
        &app_pool,
        organization_id,
        member_id,
        SavedListScope::Personal,
        "Inactive mutation queue",
    )
    .await;
    assert!(
        enable(
            &app_pool,
            organization_id,
            member_id,
            retained.list.id,
            retained.list.revision,
        )
        .await
        .unwrap()
        .changed
    );

    let router = build_router(&migrator_pool).await;
    let admin_cookie =
        login_cookie(&router, "admin@today-source-membership-lifecycle.test", PW).await;
    let deactivated = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{member_id}/status"),
        &admin_cookie,
        json!({ "status": "inactive" }),
    )
    .await;
    assert_eq!(deactivated.status(), StatusCode::OK);

    assert!(matches!(
        enable(
            &app_pool,
            organization_id,
            member_id,
            blocked.list.id,
            blocked.list.revision,
        )
        .await,
        Err(SavedListError::Unauthenticated)
    ));
    let retained_row_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(member_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(retained_row_count, 1);

    let reactivated = put_json_with_cookie(
        &router,
        &format!("/api/organization/members/{member_id}/status"),
        &admin_cookie,
        json!({ "status": "active" }),
    )
    .await;
    assert_eq!(reactivated.status(), StatusCode::OK);
    let restored = list_sources(&app_pool, organization_id, member_id, Role::Member).await;
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].list_id, retained.list.id);
    assert!(
        enable(
            &app_pool,
            organization_id,
            member_id,
            blocked.list.id,
            blocked.list.revision,
        )
        .await
        .unwrap()
        .changed
    );
}

/// The app role can write a real preference only through the migration's
/// intended INSERT/DELETE grants, while its composite tenant keys reject a
/// foreign member or a foreign saved definition.
#[sqlx::test]
#[ignore]
async fn today_source_schema_enforces_composite_tenants_and_app_role_grants(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source FK contract",
        "owner@today-source-fk-contract.test",
        "Owner",
        PW,
    )
    .await;
    let (foreign_organization_id, foreign_owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Foreign Today source FK contract",
        "owner@foreign-today-source-fk-contract.test",
        "Foreign owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let local_list = create_list(
        &app_pool,
        organization_id,
        owner_id,
        SavedListScope::Personal,
        "Local FK queue",
    )
    .await;
    let foreign_list = create_list(
        &app_pool,
        foreign_organization_id,
        foreign_owner_id,
        SavedListScope::Personal,
        "Foreign FK queue",
    )
    .await;

    sqlx::query(
        "INSERT INTO today_work_source (organization_id, user_id, list_id) VALUES ($1, $2, $3)",
    )
    .bind(organization_id)
    .bind(owner_id)
    .bind(local_list.list.id.as_uuid())
    .execute(&app_pool)
    .await
    .expect("crm_app needs the positive INSERT grant for source commands");
    let deleted = sqlx::query(
        "DELETE FROM today_work_source WHERE organization_id = $1 AND user_id = $2 AND list_id = $3",
    )
    .bind(organization_id)
    .bind(owner_id)
    .bind(local_list.list.id.as_uuid())
    .execute(&app_pool)
    .await
    .expect("crm_app needs the positive DELETE grant for source commands");
    assert_eq!(deleted.rows_affected(), 1);

    assert!(
        sqlx::query(
            "INSERT INTO today_work_source (organization_id, user_id, list_id) VALUES ($1, $2, $3)",
        )
        .bind(organization_id)
        .bind(foreign_owner_id)
        .bind(local_list.list.id.as_uuid())
        .execute(&app_pool)
        .await
        .is_err(),
        "the (organization_id, user_id) foreign key must reject a foreign member"
    );
    assert!(
        sqlx::query(
            "INSERT INTO today_work_source (organization_id, user_id, list_id) VALUES ($1, $2, $3)",
        )
        .bind(organization_id)
        .bind(owner_id)
        .bind(foreign_list.list.id.as_uuid())
        .execute(&app_pool)
        .await
        .is_err(),
        "the (organization_id, list_id) foreign key must reject a foreign definition"
    );
    assert!(
        sqlx::query("UPDATE today_work_source SET created_at = created_at WHERE false")
            .execute(&app_pool)
            .await
            .is_err(),
        "crm_app must not receive UPDATE"
    );
    assert!(
        sqlx::query("TRUNCATE today_work_source")
            .execute(&app_pool)
            .await
            .is_err(),
        "crm_app must not receive TRUNCATE"
    );
}

/// An older 011b binary could tombstone lists without cleaning source rows.
/// Exactly five such retained rows must disappear from configuration and leave
/// every source slot usable when the 011c application is restored.
#[sqlx::test]
#[ignore]
async fn five_rollback_tombstones_are_invisible_and_leave_all_five_source_slots_available(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source rollback tombstones",
        "owner@today-source-rollback-tombstones.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let mut old_ids = Vec::new();
    for index in 1..=5 {
        let list = create_list(
            &app_pool,
            organization_id,
            owner_id,
            SavedListScope::Personal,
            &format!("Rollback-era queue {index}"),
        )
        .await;
        assert!(
            enable(
                &app_pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
            .unwrap()
            .changed
        );
        old_ids.push(list.list.id.as_uuid());
    }
    let old_preference_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(owner_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(old_preference_count, 5);
    sqlx::query(
        "UPDATE saved_list \
         SET name = NULL, filter = NULL, revision = revision + 1, \
             deleted_at = now(), updated_at = now() \
         WHERE organization_id = $1 AND id = ANY($2)",
    )
    .bind(organization_id)
    .bind(&old_ids)
    .execute(&migrator_pool)
    .await
    .unwrap();

    let router = build_router(&migrator_pool).await;
    let owner_cookie =
        login_cookie(&router, "owner@today-source-rollback-tombstones.test", PW).await;
    let empty_config = get_with_cookie(&router, "/api/today/sources", &owner_cookie).await;
    assert_eq!(empty_config.status(), StatusCode::OK);
    assert_eq!(
        body_json(empty_config).await,
        json!({ "limit": 5, "sources": [] })
    );
    let dormant_preference_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(owner_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(
        dormant_preference_count, 5,
        "configuration reads must not delete rollback-era rows"
    );

    let mut new_ids = Vec::new();
    for index in 1..=5 {
        let list = create_list(
            &app_pool,
            organization_id,
            owner_id,
            SavedListScope::Personal,
            &format!("Restored queue {index}"),
        )
        .await;
        assert!(
            enable(
                &app_pool,
                organization_id,
                owner_id,
                list.list.id,
                list.list.revision,
            )
            .await
            .unwrap()
            .changed
        );
        new_ids.push(list.list.id.to_string());
    }
    new_ids.sort();
    let restored_config = get_with_cookie(&router, "/api/today/sources", &owner_cookie).await;
    assert_eq!(restored_config.status(), StatusCode::OK);
    let restored_config = body_json(restored_config).await;
    assert_eq!(restored_config["limit"], json!(5));
    let mut configured_ids: Vec<String> = restored_config["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["list_id"].as_str().unwrap().to_string())
        .collect();
    configured_ids.sort();
    assert_eq!(configured_ids, new_ids);
    let raw_preference_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM today_work_source WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(owner_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(raw_preference_count, 10);
}

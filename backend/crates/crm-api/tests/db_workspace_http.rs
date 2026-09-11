//! Slice 010c HTTP guard acceptance. Synthetic domain rows are created through
//! the normal HTTP commands while operational. The migrator then sets review
//! mode solely as a negative fixture: this is NOT proof of legal import entry.
//! Actual empty-workspace confirmation/races are covered by the import suite.
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use crm_api::{
    domain::admin::{MembershipStatus, Role},
    operator::OperatorRuntime,
    realtime::Publisher,
    state::AppState,
};
use crm_operator::{Limits, ScriptedProvider};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::common::{
    body_json, extract_cookie, get_with_cookie, login, login_cookie, post_json_with_cookie,
};

const PW: &str = "synthetic workspace guard password";
const ADMIN: &str = "workspace-admin@synthetic.test";
const MEMBER: &str = "workspace-member@synthetic.test";

struct Fixture {
    app: Router,
    org: Uuid,
    admin_id: Uuid,
    member_id: Uuid,
    admin: String,
    member: String,
    person: Uuid,
    stage: Uuid,
    publisher: Publisher,
    provider: ScriptedProvider,
}

async fn checked(response: Response, status: StatusCode) -> Value {
    assert_eq!(response.status(), status);
    body_json(response).await
}

async fn request(f: &Fixture, cookie: &str, method: Method, uri: &str, body: Value) -> Response {
    f.app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn held(response: Response, seam: &str) {
    assert_eq!(response.status(), StatusCode::CONFLICT, "{seam}");
    assert_eq!(
        body_json(response).await["error"],
        "workspace_in_migration_review",
        "{seam}"
    );
}

async fn fixture(migrator: &PgPool) -> Fixture {
    let org = crate::common::create_org(migrator, "Synthetic workspace HTTP").await;
    let admin_id = crate::common::create_user(migrator, ADMIN, "Synthetic Admin", PW).await;
    let member_id = crate::common::create_user(migrator, MEMBER, "Synthetic Member", PW).await;
    for (user, role) in [(admin_id, Role::Admin), (member_id, Role::Member)] {
        crate::common::add_membership_with(migrator, org, user, role, MembershipStatus::Active)
            .await;
    }
    let pool = crate::common::connect_as_app(migrator).await;
    let publisher = Publisher::recording();
    let provider = ScriptedProvider::new(vec![]);
    let runtime = OperatorRuntime::with_provider(Arc::new(provider.clone()), Limits::default(), 4);
    let app = crm_api::build_app(
        AppState::for_tests(pool, &crate::common::test_config(), publisher.clone())
            .with_operator(runtime),
    );
    let admin = login_cookie(&app, ADMIN, PW).await;
    let member = login_cookie(&app, MEMBER, PW).await;
    let body = checked(crate::common::post_inquiry(&app, &admin, "synthetic", json!({
        "first_name":"Synthetic", "last_name":"Person", "email":"person@workspace.synthetic", "phone":"+12025550101"
    }), Some(admin_id)).await, StatusCode::CREATED).await;
    let person = Uuid::parse_str(body["person_id"].as_str().unwrap()).unwrap();
    let stage =
        sqlx::query_scalar("SELECT stage_id FROM person WHERE id=$1 AND organization_id=$2")
            .bind(person)
            .bind(org)
            .fetch_one(migrator)
            .await
            .unwrap();
    Fixture {
        app,
        org,
        admin_id,
        member_id,
        admin,
        member,
        person,
        stage,
        publisher,
        provider,
    }
}

async fn enter_fixture_review(migrator: &PgPool, org: Uuid) {
    // Test-only negative state, intentionally no import binding. Production has
    // no route to toggle mode and may enter only after proving an empty target.
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(org).execute(migrator).await.unwrap();
}

async fn event_count(f: &Fixture) -> usize {
    let Publisher::Recording(records, _) = &f.publisher else {
        panic!("recording fixture")
    };
    records.lock().await.len()
}

#[sqlx::test]
#[ignore]
async fn workspace_http_review_reads_require_current_admin_and_clear_session_status(
    migrator: PgPool,
) {
    let f = fixture(&migrator).await;
    let paths = [
        "/api/people".to_owned(),
        format!("/api/people/{}", f.person),
        "/api/stages".into(),
        "/api/tags".into(),
        "/api/custom-fields".into(),
        "/api/saved-lists".into(),
        "/api/tasks?scope=mine".into(),
        "/api/inquiry-sources".into(),
    ];
    for path in &paths {
        for cookie in [&f.admin, &f.member] {
            assert_eq!(
                get_with_cookie(&f.app, path, cookie).await.status(),
                StatusCode::OK,
                "operational {path}"
            );
        }
    }
    let before = checked(
        get_with_cookie(&f.app, "/api/me", &f.member).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(before["organization"]["workspace_mode"], "operational");
    assert_eq!(before["organization"]["workspace_revision"], "1");
    enter_fixture_review(&migrator, f.org).await;
    for path in &paths {
        held(get_with_cookie(&f.app, path, &f.member).await, path).await;
        assert_eq!(
            get_with_cookie(&f.app, path, &f.admin).await.status(),
            StatusCode::OK,
            "review admin {path}"
        );
    }
    for cookie in [&f.admin, &f.member] {
        let me = checked(
            get_with_cookie(&f.app, "/api/me", cookie).await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(me["organization"]["workspace_mode"], "migration_review");
        assert_eq!(me["organization"]["workspace_revision"], "2");
    }
    let relogin = checked(login(&f.app, MEMBER, PW).await, StatusCode::OK).await;
    assert_eq!(
        relogin["organization"]["workspace_mode"],
        "migration_review"
    );
    let other = crate::common::create_org(&migrator, "Foreign synthetic workspace").await;
    let foreign_user =
        crate::common::create_user(&migrator, "foreign@workspace.synthetic", "Foreign", PW).await;
    crate::common::add_membership(&migrator, other, foreign_user).await;
    let foreign_cookie = login_cookie(&f.app, "foreign@workspace.synthetic", PW).await;
    assert_eq!(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{}", f.person),
            &foreign_cookie
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    // A still-valid cookie is insufficient after the role changes in another tab.
    checked(
        request(
            &f,
            &f.admin,
            Method::PUT,
            &format!("/api/organization/members/{}/role", f.member_id),
            json!({"role":"admin"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        get_with_cookie(&f.app, "/api/people", &f.member)
            .await
            .status(),
        StatusCode::OK
    );
    checked(
        request(
            &f,
            &f.member,
            Method::PUT,
            &format!("/api/organization/members/{}/role", f.admin_id),
            json!({"role":"member"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    held(
        get_with_cookie(&f.app, "/api/people", &f.admin).await,
        "demoted existing cookie",
    )
    .await;
    assert_eq!(
        crate::common::delete_with_cookie(&f.app, "/api/session", &f.admin)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get_with_cookie(&f.app, "/api/me", &f.admin).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
#[ignore]
async fn workspace_http_today_operator_realtime_cannot_escape_review(migrator: PgPool) {
    let f = fixture(&migrator).await;
    for cookie in [&f.admin, &f.member] {
        assert_eq!(
            get_with_cookie(&f.app, "/api/today", cookie).await.status(),
            StatusCode::OK
        );
        assert_eq!(
            post_json_with_cookie(&f.app, "/api/realtime/token", cookie, json!({}))
                .await
                .status(),
            StatusCode::OK
        );
    }
    enter_fixture_review(&migrator, f.org).await;
    let events = event_count(&f).await;
    for cookie in [&f.admin, &f.member] {
        for path in ["/api/today", "/api/today/feeds", "/api/today/sources"] {
            held(get_with_cookie(&f.app, path, cookie).await, path).await;
        }
        held(
            post_json_with_cookie(&f.app, "/api/realtime/token", cookie, json!({})).await,
            "realtime mint",
        )
        .await;
        held(
            post_json_with_cookie(
                &f.app,
                "/api/operator/turns",
                cookie,
                json!({"message":"Read my people"}),
            )
            .await,
            "Operator admission",
        )
        .await;
    }
    assert!(
        f.provider.requests().is_empty(),
        "no inference was scheduled"
    );
    assert_eq!(event_count(&f).await, events, "no outbound notifications");
    let admissions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_operation_admission WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(admissions, 0);
}

#[sqlx::test]
#[ignore]
async fn workspace_http_blocks_alternate_person_note_task_and_tag_mutations(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let person_path = format!("/api/people/{}", f.person);
    let note = checked(
        post_json_with_cookie(
            &f.app,
            &format!("{person_path}/notes"),
            &f.admin,
            json!({"body":"Retain this synthetic note"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let task = checked(
        post_json_with_cookie(
            &f.app,
            &format!("{person_path}/tasks"),
            &f.admin,
            json!({"title":"Retain this synthetic task","kind":"follow_up"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let tag = checked(
        post_json_with_cookie(
            &f.app,
            "/api/tags",
            &f.admin,
            json!({"name":"Synthetic tag"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let note_path = format!(
        "{person_path}/notes/{}",
        note["note"]["id"].as_str().unwrap()
    );
    let task_path = format!(
        "{person_path}/tasks/{}",
        task["task"]["id"].as_str().unwrap()
    );
    let tag_id = tag["tag"]["id"].as_str().unwrap();
    let person_tag = format!("{person_path}/tags/{tag_id}");
    checked(
        request(&f, &f.admin, Method::PUT, &person_tag, json!({})).await,
        StatusCode::OK,
    )
    .await;
    let detail_before = checked(
        get_with_cookie(&f.app, &person_path, &f.admin).await,
        StatusCode::OK,
    )
    .await;
    let events = event_count(&f).await;
    enter_fixture_review(&migrator, f.org).await;
    let attempts = [
        (
            Method::POST,
            format!("{person_path}/assignment"),
            json!({"assigned_user_id":null}),
        ),
        (
            Method::POST,
            format!("{person_path}/stage"),
            json!({"stage_id":f.stage}),
        ),
        (
            Method::POST,
            format!("{person_path}/contact-attempts"),
            json!({"channel":"call","outcome":"no_answer"}),
        ),
        (
            Method::POST,
            format!("{person_path}/notes"),
            json!({"body":"Must not appear"}),
        ),
        (
            Method::PUT,
            note_path.clone(),
            json!({"body":"Must not replace"}),
        ),
        (Method::DELETE, note_path, json!({})),
        (
            Method::POST,
            format!("{person_path}/tasks"),
            json!({"title":"Must not appear"}),
        ),
        (
            Method::PUT,
            task_path.clone(),
            json!({"title":"Must not replace","kind":"other","due_at":null,"assignee_user_id":f.admin_id}),
        ),
        (Method::POST, format!("{task_path}/complete"), json!({})),
        (Method::POST, format!("{task_path}/reopen"), json!({})),
        (
            Method::POST,
            format!("{task_path}/snooze"),
            json!({"due_at":"2099-01-01T00:00:00Z"}),
        ),
        (Method::DELETE, task_path, json!({})),
        (Method::PUT, person_tag.clone(), json!({})),
        (Method::DELETE, person_tag, json!({})),
        (
            Method::POST,
            "/api/tags".into(),
            json!({"name":"Must not appear"}),
        ),
        (
            Method::PUT,
            format!("/api/tags/{tag_id}"),
            json!({"name":"Must not rename"}),
        ),
        (Method::DELETE, format!("/api/tags/{tag_id}"), json!({})),
    ];
    for (method, path, body) in attempts {
        held(request(&f, &f.admin, method, &path, body).await, &path).await;
    }
    assert_eq!(
        checked(
            get_with_cookie(&f.app, &person_path, &f.admin).await,
            StatusCode::OK
        )
        .await,
        detail_before,
        "blocked edits preserve the complete Person response"
    );
    assert_eq!(event_count(&f).await, events);
}

#[sqlx::test]
#[ignore]
async fn workspace_http_settings_provisioning_and_governance_remain_distinct(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let field = checked(
        post_json_with_cookie(
            &f.app,
            "/api/custom-fields",
            &f.admin,
            json!({"label":"Synthetic choice","field_type":"choice","options":["One"]}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let field_id = field["field"]["id"].as_str().unwrap();
    let option_id = field["field"]["options"][0]["id"].as_str().unwrap();
    let list = checked(post_json_with_cookie(&f.app, "/api/saved-lists", &f.admin,
        json!({"request_id":Uuid::new_v4(),"scope":"shared","name":"Synthetic list","filter":{"version":1,"clauses":[]}})).await, StatusCode::CREATED).await;
    let list_id = list["list"]["id"].as_str().unwrap();
    let person_field = format!("/api/people/{}/custom-fields/{field_id}", f.person);
    checked(
        request(
            &f,
            &f.admin,
            Method::PUT,
            &person_field,
            json!({"value":{"option_id":option_id}}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let fields_before = checked(
        get_with_cookie(&f.app, "/api/custom-fields", &f.admin).await,
        StatusCode::OK,
    )
    .await;
    let lists_before = checked(
        get_with_cookie(&f.app, "/api/saved-lists", &f.admin).await,
        StatusCode::OK,
    )
    .await;
    let settings_before = checked(
        get_with_cookie(&f.app, "/api/organization/intake-settings", &f.admin).await,
        StatusCode::OK,
    )
    .await;
    enter_fixture_review(&migrator, f.org).await;
    for (method, path, body) in [
        (
            Method::POST,
            "/api/custom-fields".to_owned(),
            json!({"label":"Must not appear","field_type":"text"}),
        ),
        (
            Method::PUT,
            format!("/api/custom-fields/{field_id}"),
            json!({"label":"Must not rename","archived":true}),
        ),
        (
            Method::PUT,
            "/api/custom-fields/order".into(),
            json!({"field_ids":[field_id]}),
        ),
        (
            Method::POST,
            format!("/api/custom-fields/{field_id}/options"),
            json!({"label":"Must not appear"}),
        ),
        (
            Method::PUT,
            format!("/api/custom-fields/{field_id}/options/{option_id}"),
            json!({"label":"Must not rename","archived":true}),
        ),
        (
            Method::PUT,
            person_field.clone(),
            json!({"value":{"option_id":option_id}}),
        ),
        (Method::DELETE, person_field, json!({})),
        (
            Method::POST,
            "/api/saved-lists".into(),
            json!({"request_id":Uuid::new_v4(),"scope":"personal","name":"Must not appear","filter":{"version":1,"clauses":[]}}),
        ),
        (
            Method::PUT,
            format!("/api/saved-lists/{list_id}"),
            json!({"expected_revision":1,"name":"Must not replace","filter":{"version":1,"clauses":[]}}),
        ),
        (
            Method::DELETE,
            format!("/api/saved-lists/{list_id}"),
            json!({"expected_revision":1}),
        ),
        (
            Method::PUT,
            "/api/organization/intake-settings".into(),
            json!({"intake_routing_mode":"default_assignee","intake_default_assignee_user_id":f.member_id}),
        ),
        (
            Method::POST,
            "/api/organization/intake-address/rotate".into(),
            json!({}),
        ),
        (
            Method::POST,
            "/api/capture/address/rotate".into(),
            json!({}),
        ),
    ] {
        held(request(&f, &f.admin, method, &path, body).await, &path).await;
    }
    // Bare fixture membership has no capture address. GET's usual self-healing
    // write must be stopped; invitation acceptance may still provision one.
    held(
        get_with_cookie(&f.app, "/api/capture/address", &f.admin).await,
        "capture GET provisioning",
    )
    .await;
    assert_eq!(
        checked(
            get_with_cookie(&f.app, "/api/custom-fields", &f.admin).await,
            StatusCode::OK
        )
        .await,
        fields_before
    );
    assert_eq!(
        checked(
            get_with_cookie(&f.app, "/api/saved-lists", &f.admin).await,
            StatusCode::OK
        )
        .await,
        lists_before
    );
    assert_eq!(
        checked(
            get_with_cookie(&f.app, "/api/organization/intake-settings", &f.admin).await,
            StatusCode::OK
        )
        .await,
        settings_before
    );
    let invite = checked(
        post_json_with_cookie(
            &f.app,
            "/api/organization/invitations",
            &f.admin,
            json!({"email":"new-review-member@synthetic.test","role":"member"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let token = invite["accept_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/invite/")
        .unwrap();
    let accepted = post_json_with_cookie(
        &f.app,
        "/api/invitations/accept",
        "",
        json!({"token":token,"display_name":"Review Member","password":PW}),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    let cookie = extract_cookie(&accepted);
    let accepted = body_json(accepted).await;
    assert_eq!(
        accepted["organization"]["workspace_mode"],
        "migration_review"
    );
    held(
        get_with_cookie(&f.app, "/api/people", &cookie).await,
        "new member also waits",
    )
    .await;
    let capture_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM capture_address WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(
        capture_count, 1,
        "governance acceptance provisions exactly its new member"
    );
    checked(
        request(
            &f,
            &f.admin,
            Method::PUT,
            &format!("/api/organization/members/{}/status", f.member_id),
            json!({"status":"inactive"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        get_with_cookie(&f.app, "/api/me", &f.member).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
#[ignore]
async fn workspace_http_holds_existing_intake_retry_discard_and_today_configuration(
    migrator: PgPool,
) {
    let f = fixture(&migrator).await;
    let unresolved = checked(
        crate::common::post_inquiry(
            &f.app,
            &f.admin,
            "synthetic",
            json!({"first_name":"No contact"}),
            None,
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(unresolved["status"], "unresolved");
    let unresolved_path = format!(
        "/api/intake/unresolved/{}",
        unresolved["raw_payload_id"].as_str().unwrap()
    );
    let before = checked(
        get_with_cookie(&f.app, &unresolved_path, &f.admin).await,
        StatusCode::OK,
    )
    .await;
    let feeds = checked(
        get_with_cookie(&f.app, "/api/organization/today-feeds", &f.admin).await,
        StatusCode::OK,
    )
    .await;
    let feed = &feeds["feeds"][0];
    let feed_path = format!(
        "/api/organization/today-feeds/{}",
        feed["feed_key"].as_str().unwrap()
    );
    let list = checked(post_json_with_cookie(&f.app, "/api/saved-lists", &f.admin,
        json!({"request_id":Uuid::new_v4(),"scope":"personal","name":"Synthetic source","filter":{"version":1,"clauses":[]}})).await, StatusCode::CREATED).await;
    let source_path = format!(
        "/api/today/sources/{}",
        list["list"]["id"].as_str().unwrap()
    );
    checked(
        request(
            &f,
            &f.admin,
            Method::PUT,
            &source_path,
            json!({"expected_list_revision":1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    enter_fixture_review(&migrator, f.org).await;
    let events = event_count(&f).await;
    for (method, path, body) in [
        (Method::POST, format!("{unresolved_path}/retry"), json!({})),
        (
            Method::POST,
            format!("{unresolved_path}/discard"),
            json!({}),
        ),
        (
            Method::PUT,
            source_path.clone(),
            json!({"expected_list_revision":1}),
        ),
        (Method::DELETE, source_path, json!({})),
        (
            Method::PUT,
            feed_path.clone(),
            json!({"expected_revision":feed["revision"],"filter":feed["filter"],"fresh_within_hours":feed["fresh_within_hours"]}),
        ),
        (
            Method::POST,
            format!("{feed_path}/revert"),
            json!({"expected_revision":feed["revision"]}),
        ),
        (
            Method::PUT,
            format!("{feed_path}/enabled"),
            json!({"expected_revision":feed["revision"],"enabled":false}),
        ),
        (
            Method::POST,
            format!("{feed_path}/preview"),
            json!({"filter":feed["filter"],"fresh_within_hours":feed["fresh_within_hours"]}),
        ),
    ] {
        held(request(&f, &f.admin, method, &path, body).await, &path).await;
    }
    assert_eq!(
        checked(
            get_with_cookie(&f.app, &unresolved_path, &f.admin).await,
            StatusCode::OK
        )
        .await,
        before
    );
    assert_eq!(event_count(&f).await, events);
}

//! 010c HTTP acceptance driven by real synthetic retained captures. No worker
//! is spawned in the fixture: each preparation/execution step is explicit, so
//! confirmations, replays and cancellation are observable without sleeps.
use crate::import_support as support;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};
use tower::ServiceExt;
use uuid::Uuid;

use crate::common::{body_json, get_with_cookie, post_json_with_cookie};
use support::{drain_import, fixture, Fixture};

const ROOT: &str = "/api/migrations/fub/imports";

fn people() -> Vec<Value> {
    vec![
        json!({"id":11,"firstName":"Synthetic","lastName":"One","stage":"Lead","assignedUserId":3,"emails":[{"value":"shared@import.synthetic"}],"phones":[{"value":"+12025550111"}]}),
        json!({"id":12,"firstName":"Synthetic","lastName":"Two","stage":"Lead","assignedUserId":3,"emails":[{"value":"SHARED@import.synthetic"}],"phones":[{"value":"2025550111"}]}),
        json!({"id":13,"firstName":"Synthetic Trash","stage":"Lead","isTrash":true}),
    ]
}

async fn checked(response: Response, expected: StatusCode) -> Value {
    let actual = response.status();
    let headers = response.headers().clone();
    let value = body_json(response).await;
    assert_eq!(actual, expected, "closed error: {}", value["error"]);
    if expected.is_success() {
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
        let rendered = value.to_string();
        for secret in [
            "credential_ciphertext",
            "credential_nonce",
            "patch_ciphertext",
            "provenance_nonce",
        ] {
            assert!(
                !rendered.contains(secret),
                "private storage field escaped its boundary"
            );
        }
    }
    value
}

async fn detail(f: &Fixture, id: Uuid) -> Value {
    checked(
        get_with_cookie(&f.app, &format!("{ROOT}/{id}"), &f.cookie).await,
        StatusCode::OK,
    )
    .await
}

fn proposal(f: &Fixture, request_id: Uuid) -> Value {
    json!({"request_id":request_id,"snapshot_id":f.snapshot,"preview_id":f.preview})
}

async fn propose(f: &Fixture) -> (Uuid, Uuid) {
    let value = checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, proposal(f, Uuid::new_v4())).await,
        StatusCode::CREATED,
    )
    .await;
    (
        Uuid::parse_str(value["import_id"].as_str().unwrap()).unwrap(),
        Uuid::parse_str(value["plan_id"].as_str().unwrap()).unwrap(),
    )
}

async fn patch(f: &Fixture, id: Uuid, revision: &str, stages: Value, assignees: Value) -> Uuid {
    let value = checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
        json!({"request_id":Uuid::new_v4(),"expected_plan_revision":revision,"stage_mappings":stages,"assignee_mappings":assignees})).await, StatusCode::ACCEPTED).await;
    Uuid::parse_str(value["plan_id"].as_str().unwrap()).unwrap()
}

async fn ready(f: &Fixture) -> (Uuid, Value) {
    let (id, _) = propose(f).await;
    drain_import(f).await;
    let first = detail(f, id).await;
    assert_eq!(first["plan"]["state"], "ready");
    patch(
        f,
        id,
        first["plan"]["revision"].as_str().unwrap(),
        json!([{"source_key":"4","choice":{"kind":"existing","stage_id":f.lead_stage}}]),
        json!([{"source_key":"3","choice":{"kind":"member","user_id":f.actor}}]),
    )
    .await;
    drain_import(f).await;
    let result = detail(f, id).await;
    assert_eq!(result["plan"]["state"], "ready");
    (id, result)
}

fn confirmation(value: &Value) -> Value {
    json!({"request_id":Uuid::new_v4(),"plan_id":value["plan"]["id"],"plan_revision":value["plan"]["revision"],
        "confirmation_digest":value["plan"]["confirmation_digest"],"acknowledgments":{
            "held_count":value["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}})
}

/// Server-owned, synthetic readiness evidence; no tenant acknowledgment or
/// runtime bypass participates in this HTTP replay regression.
struct ReleaseReport(PathBuf);

impl ReleaseReport {
    fn write(&self, value: &Value) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&self.0)
            .unwrap();
        file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
    }
}

impl Drop for ReleaseReport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn page(f: &Fixture, path: &str, key: &str) -> Value {
    let response = get_with_cookie(&f.app, path, &f.cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.len() <= 512 * 1024, "bounded review page");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value[key].is_array());
    value
}

#[sqlx::test]
#[ignore]
async fn import_http_creation_is_strict_scoped_idempotent_and_source_free(migrator: PgPool) {
    let f = fixture(&migrator, people()).await;
    let calls = f.reader.calls();
    let body = proposal(&f, Uuid::new_v4());
    checked(
        post_json_with_cookie(&f.app, ROOT, "", body.clone()).await,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    checked(
        post_json_with_cookie(&f.app, ROOT, &f.member_cookie, body.clone()).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let mut forged = body.clone();
    forged["organization_id"] = json!(f.org);
    checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, forged).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let value = checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, body.clone()).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, ROOT, &f.cookie, body.clone()).await,
            StatusCode::CREATED
        )
        .await,
        value
    );
    let mut changed = body;
    changed["preview_id"] = json!(Uuid::new_v4());
    checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, changed).await,
        StatusCode::CONFLICT,
    )
    .await;
    let id = Uuid::parse_str(value["import_id"].as_str().unwrap()).unwrap();
    let plan = value["plan_id"].as_str().unwrap();
    let busy = checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
        json!({"request_id":Uuid::new_v4(),"expected_plan_revision":"1","stage_mappings":[],"assignee_mappings":[]})).await, StatusCode::CONFLICT).await;
    assert_eq!(busy["error"], "import_busy");
    drain_import(&f).await;
    assert_eq!(f.reader.calls(), calls, "retained-only preparation");
    let detail = detail(&f, id).await;
    assert_eq!(detail["plan"]["id"], plan);
    assert_eq!(detail["plan"]["counts"]["source_people"], "3");
    assert_eq!(detail["plan"]["counts"]["eligible_people"], "0");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(count, 0, "planning performs no business writes");
    let oversized = format!(
        "{{\"request_id\":\"{}\",\"extra\":\"{}\"}}",
        Uuid::new_v4(),
        "x".repeat(65_536)
    );
    let response = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(ROOT)
                .header("cookie", &f.cookie)
                .header("content-type", "application/json")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();
    checked(response, StatusCode::PAYLOAD_TOO_LARGE).await;
    for suffix in ["?limit=0", "?limit=51", "?cursor=invalid"] {
        checked(
            get_with_cookie(&f.app, &format!("{ROOT}{suffix}"), &f.cookie).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
}

#[sqlx::test]
#[ignore]
async fn import_http_partial_patches_are_frozen_and_confirmation_replays_without_duplicate_people(
    migrator: PgPool,
) {
    let f = fixture(&migrator, people()).await;
    let calls = f.reader.calls();
    let (id, original_plan) = propose(&f).await;
    drain_import(&f).await;
    let stage_plan = patch(
        &f,
        id,
        "1",
        json!([{"source_key":"4","choice":{"kind":"existing","stage_id":f.lead_stage}}]),
        json!([]),
    )
    .await;
    drain_import(&f).await;
    let stage_only = detail(&f, id).await;
    assert_eq!(stage_only["plan"]["counts"]["eligible_people"], "0");
    let plan = patch(
        &f,
        id,
        "2",
        json!([]),
        json!([{"source_key":"3","choice":{"kind":"member","user_id":f.actor}}]),
    )
    .await;
    drain_import(&f).await;
    let ready = detail(&f, id).await;
    assert_eq!(ready["plan"]["counts"]["eligible_people"], "2");
    assert_eq!(ready["plan"]["counts"]["held_people"], "1");
    assert_eq!(ready["plan"]["counts"]["contacts"], "4");
    assert_eq!(ready["plan"]["revision"], "3");
    assert_ne!(original_plan, stage_plan);
    assert_ne!(stage_plan, plan);
    let mappings = page(
        &f,
        &format!("{ROOT}/{id}/plans/{plan}/mappings?kind=stage&limit=1"),
        "mappings",
    )
    .await;
    assert_eq!(
        mappings["mappings"][0]["choice"]["kind"], "existing",
        "untouched choice inherits"
    );
    let records_path = format!("{ROOT}/{id}/plans/{plan}/records?limit=1");
    let records = page(&f, &records_path, "records").await;
    assert_eq!(records["records"].as_array().unwrap().len(), 1);
    let cursor = records["next_cursor"].as_str().unwrap();
    // Authenticated cursors cannot be transferred to another frozen revision.
    checked(
        get_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/plans/{stage_plan}/records?limit=1&cursor={cursor}"),
            &f.cookie,
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let confirm = confirmation(&ready);
    let mut stale = confirm.clone();
    stale["plan_id"] = json!(stage_plan);
    checked(
        post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/confirm"), &f.cookie, stale).await,
        StatusCode::CONFLICT,
    )
    .await;
    let mut unchecked = confirm.clone();
    unchecked["acknowledgments"]["review_only"] = json!(false);
    checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/confirm"),
            &f.cookie,
            unchecked,
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    let accepted = checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/confirm"),
            &f.cookie,
            confirm.clone(),
        )
        .await,
        StatusCode::ACCEPTED,
    )
    .await;
    assert_eq!(accepted["workspace_mode"], "migration_review");
    drain_import(&f).await;
    let replay = checked(
        post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/confirm"), &f.cookie, confirm).await,
        StatusCode::ACCEPTED,
    )
    .await;
    assert_eq!(replay["receipt"], accepted["receipt"]);
    let completed = detail(&f, id).await;
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed["counts"]["imported_people"], "2");
    assert_eq!(completed["counts"]["imported_contacts"], "4");
    assert_eq!(f.reader.calls(), calls);
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let facts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM person_imported WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap();
    let inquiries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM inquiry WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!((people, facts, inquiries), (2, 2, 0));
    let results = page(
        &f,
        &format!("{ROOT}/{id}/results?disposition=imported&limit=1"),
        "results",
    )
    .await;
    let person = results["results"][0]["person_id"].as_str().unwrap();
    let provenance = checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{person}/import-provenance"),
            &f.cookie,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(provenance.to_string().contains(&id.to_string()));
    // Migration keeps its admin-only 403 contract in review mode. The generic
    // business-read hold must not replace that authorization response with 409.
    for path in [
        "/api/migrations/fub/".to_owned(),
        "/api/migrations/fub/snapshots".to_owned(),
        ROOT.to_owned(),
        format!("{ROOT}/{id}"),
        format!("{ROOT}/{id}/plans/{plan}/records"),
        format!("{ROOT}/{id}/results"),
        format!("/api/people/{person}/import-provenance"),
        format!("/api/people/{person}/import-provenance/fields/sourceUrl"),
    ] {
        let denied = checked(
            get_with_cookie(&f.app, &path, &f.member_cookie).await,
            StatusCode::FORBIDDEN,
        )
        .await;
        assert_eq!(denied["error"], "forbidden", "{path}");
    }
    checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
        json!({"request_id":Uuid::new_v4(),"expected_plan_revision":"3","stage_mappings":[],"assignee_mappings":[]})).await, StatusCode::CONFLICT).await;
    assert_eq!(
        get_with_cookie(&f.app, "/api/people", &f.member_cookie)
            .await
            .status(),
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
#[ignore]
async fn import_http_confirmation_replays_after_release_report_expires_or_disappears(
    migrator: PgPool,
) {
    use crm_app::auth::workspace;

    let mut f = fixture(&migrator, people()).await;
    let (id, ready) = ready(&f).await;
    // A second, still operational Organization proves a genuinely new
    // confirmation is rejected when current readiness is absent.
    let second = fixture(&migrator, people()).await;
    let (second_id, second_ready) = self::ready(&second).await;
    let report = ReleaseReport(
        std::env::temp_dir().join(format!("crm-010c-http-release-{}.json", Uuid::new_v4())),
    );
    let now = chrono::Utc::now();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&migrator)
        .await
        .unwrap();
    let mut evidence = json!({
        "confirmation_ready":true,
        "database_name":database,
        "checked_at":now,
        "evidence_expires_at":now + chrono::Duration::minutes(5),
        "candidates":[{"sha256":workspace::artifact_fingerprint().await.unwrap(),
            "gate_version":workspace::GATE_VERSION}]
    });
    report.write(&evidence);
    let mut state = crm_api::state::AppState::for_tests(
        f.pool.clone(),
        &crate::common::test_config(),
        crm_api::realtime::Publisher::recording(),
    )
    .with_migration_reader(f.reader.clone());
    state.import_release_path = Some(report.0.clone());
    assert!(state.current_import_release().await.is_some());
    f.app = crm_api::build_app(state.clone());
    let path = format!("{ROOT}/{id}/confirm");
    let command = confirmation(&ready);
    let original = checked(
        post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()).await,
        StatusCode::ACCEPTED,
    )
    .await;
    evidence["evidence_expires_at"] = json!(now - chrono::Duration::seconds(1));
    report.write(&evidence);
    assert!(state.current_import_release().await.is_none());
    assert_eq!(detail(&f, id).await["release_ready"], false);
    let replay = checked(
        post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()).await,
        StatusCode::ACCEPTED,
    )
    .await;
    assert_eq!(
        replay, original,
        "return the exact durable receipt envelope"
    );
    let mut altered = command.clone();
    altered["acknowledgments"]["held_count"] = json!("999");
    checked(
        post_json_with_cookie(&f.app, &path, &f.cookie, altered).await,
        StatusCode::CONFLICT,
    )
    .await;
    checked(
        post_json_with_cookie(&f.app, &path, &f.member_cookie, command.clone()).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{second_id}/confirm"),
            &second.cookie,
            confirmation(&second_ready),
        )
        .await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    std::fs::remove_file(&report.0).unwrap();
    assert!(state.current_import_release().await.is_none());
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, &path, &f.cookie, command).await,
            StatusCode::ACCEPTED,
        )
        .await,
        original
    );
    let binding_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_workspace WHERE organization_id=$1")
            .bind(second.org)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!(binding_count, 0, "new confirmation cannot enter review");
}

#[sqlx::test]
#[ignore]
async fn import_http_cancel_is_durable_and_does_not_release_review_or_replan(migrator: PgPool) {
    let f = fixture(&migrator, people()).await;
    let (id, ready) = ready(&f).await;
    checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/confirm"),
            &f.cookie,
            confirmation(&ready),
        )
        .await,
        StatusCode::ACCEPTED,
    )
    .await;
    let cancel = json!({"request_id":Uuid::new_v4()});
    let first = checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/cancel"),
            &f.cookie,
            cancel.clone(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/cancel"), &f.cookie, cancel).await,
            StatusCode::OK
        )
        .await,
        first
    );
    drain_import(&f).await;
    let committed = detail(&f, id).await;
    assert_eq!(committed["state"], "cancelled");
    for field in [
        "retained_bytes",
        "reserved_bytes",
        "cancellation_reserved_bytes",
    ] {
        assert_eq!(first["import"][field], committed[field], "settled {field}");
    }
    assert_eq!(first["import"]["reserved_bytes"], "0");
    assert_eq!(first["import"]["cancellation_reserved_bytes"], "0");
    let (retained, reserved): (i64, i64) = sqlx::query_as(
        "SELECT retained_bytes,reserved_bytes FROM migration_import WHERE id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(f.org)
    .fetch_one(&migrator)
    .await
    .unwrap();
    assert_eq!(first["import"]["retained_bytes"], retained.to_string());
    assert_eq!(reserved, 0);
    let mode: String = sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let bindings: i64 =
        sqlx::query_scalar("SELECT count(*) FROM migration_workspace WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&migrator)
            .await
            .unwrap();
    assert_eq!((mode.as_str(), bindings), ("migration_review", 1));
    checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/retry"),
            &f.cookie,
            json!({"request_id":Uuid::new_v4()}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
        json!({"request_id":Uuid::new_v4(),"expected_plan_revision":ready["plan"]["revision"],"stage_mappings":[],"assignee_mappings":[]})).await, StatusCode::CONFLICT).await;
}

#[sqlx::test]
#[ignore]
async fn import_http_foreign_resources_and_invalid_mapping_choices_fail_closed(migrator: PgPool) {
    let f = fixture(&migrator, people()).await;
    let (id, plan) = propose(&f).await;
    drain_import(&f).await;
    let other_org = crate::common::create_org(&migrator, "Foreign import HTTP").await;
    let email = "foreign-import-http@synthetic.test";
    let password = "foreign synthetic import password";
    let user = crate::common::create_user(&migrator, email, "Foreign admin", password).await;
    crate::common::add_membership_with(
        &migrator,
        other_org,
        user,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let other = crate::common::login_cookie(&f.app, email, password).await;
    for path in [
        format!("{ROOT}/{id}"),
        format!("{ROOT}/{id}/plans/{plan}/records"),
        format!("{ROOT}/{id}/plans/{plan}/mappings?kind=stage"),
        format!("{ROOT}/{id}/results"),
    ] {
        checked(
            get_with_cookie(&f.app, &path, &other).await,
            StatusCode::NOT_FOUND,
        )
        .await;
    }
    checked(
        post_json_with_cookie(&f.app, ROOT, &other, proposal(&f, Uuid::new_v4())).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    for mappings in [
        json!([{"source_key":"not-a-source-key","choice":{"kind":"hold"}}]),
        json!([{"source_key":"4","choice":{"kind":"hold"}},{"source_key":"4","choice":{"kind":"hold"}}]),
    ] {
        checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
            json!({"request_id":Uuid::new_v4(),"expected_plan_revision":"1","stage_mappings":mappings,"assignee_mappings":[]})).await, StatusCode::UNPROCESSABLE_ENTITY).await;
    }
    let fifty_one = (0..51)
        .map(|n| json!({"source_key":n.to_string(),"choice":{"kind":"hold"}}))
        .collect::<Vec<_>>();
    checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
        json!({"request_id":Uuid::new_v4(),"expected_plan_revision":"1","stage_mappings":fifty_one,"assignee_mappings":[]})).await, StatusCode::UNPROCESSABLE_ENTITY).await;
    // Source labels and source values are never client-authored command fields.
    checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie,
        json!({"request_id":Uuid::new_v4(),"expected_plan_revision":"1","stage_mappings":[{"source_key":"4","choice":{"kind":"create","name":"Injected label"}}],"assignee_mappings":[]})).await, StatusCode::BAD_REQUEST).await;
    let current = detail(&f, id).await;
    assert_eq!(current["plan"]["id"], plan.to_string());
    assert_eq!(
        current["plan"]["revision"], "1",
        "invalid patches leave the frozen plan unchanged"
    );
}

#[sqlx::test]
#[ignore]
async fn import_http_large_fields_are_bounded_exact_and_cursor_scoped(migrator: PgPool) {
    let text = "🌿\"<script>synthetic</script>\n".repeat(80_000);
    assert!(text.len() > 2 * 1024 * 1024);
    let item = json!({"id":11,"firstName":"Synthetic large field","stage":"Lead","assignedUserId":3,"sourceUrl":text});
    assert!(item.to_string().len() < 4 * 1024 * 1024 - 1024);
    let f = fixture(&migrator, vec![item]).await;
    let calls = f.reader.calls();
    let (id, ready) = ready(&f).await;
    assert_eq!(ready["plan"]["counts"]["eligible_people"], "1");
    let plan = ready["plan"]["id"].as_str().unwrap();
    let records = page(
        &f,
        &format!("{ROOT}/{id}/plans/{plan}/records?limit=50"),
        "records",
    )
    .await;
    let record = &records["records"][0];
    let field = record["proposed"]["provenance"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["label"] == "sourceUrl")
        .unwrap();
    assert_eq!(field["abbreviated"], true);
    let expected = serde_json::to_string(&text).unwrap();
    assert_eq!(field["full_utf8_bytes"], expected.len().to_string());
    let record_id = record["id"].as_str().unwrap();
    let field_key = field["field_key"].as_str().unwrap();
    let path = format!("{ROOT}/{id}/plans/{plan}/records/{record_id}/fields/{field_key}");
    for limit in [0, 1, 3, 65_537] {
        checked(
            get_with_cookie(&f.app, &format!("{path}?limit={limit}"), &f.cookie).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    let first = checked(
        get_with_cookie(&f.app, &format!("{path}?limit=4"), &f.cookie).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(first["offset"], "0");
    let first_text = first["text"].as_str().unwrap();
    assert!(!first_text.is_empty() && first_text.len() <= 4);
    let cursor = first["next_cursor"].as_str().unwrap();
    checked(get_with_cookie(&f.app, &format!("{ROOT}/{id}/plans/{plan}/records/{record_id}/fields/first_name?limit=4&cursor={cursor}"), &f.cookie).await, StatusCode::BAD_REQUEST).await;
    let mut complete = first_text.to_owned();
    let mut cursor = Some(cursor.to_owned());
    let mut pages = 1;
    while let Some(current) = cursor {
        let result = checked(
            get_with_cookie(
                &f.app,
                &format!("{path}?limit=65536&cursor={current}"),
                &f.cookie,
            )
            .await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(result["offset"], complete.len().to_string());
        let part = result["text"].as_str().unwrap();
        assert!(!part.is_empty() && part.len() <= 65_536);
        complete.push_str(part);
        cursor = result["next_cursor"].as_str().map(str::to_owned);
        pages += 1;
        assert!(pages < 100);
    }
    assert!(
        complete == expected,
        "no stored value is shortened to fit a display response"
    );
    checked(
        post_json_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/confirm"),
            &f.cookie,
            confirmation(&ready),
        )
        .await,
        StatusCode::ACCEPTED,
    )
    .await;
    drain_import(&f).await;
    let results = page(&f, &format!("{ROOT}/{id}/results?limit=1"), "results").await;
    let person = results["results"][0]["person_id"].as_str().unwrap();
    let provenance = checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{person}/import-provenance"),
            &f.cookie,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(provenance.to_string().len() < 512 * 1024);
    let retained = checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{person}/import-provenance/fields/{field_key}?limit=65536"),
            &f.cookie,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(expected.starts_with(retained["text"].as_str().unwrap()));
    let plan_cursor = first["next_cursor"].as_str().unwrap();
    checked(get_with_cookie(&f.app, &format!("/api/people/{person}/import-provenance/fields/{field_key}?limit=4&cursor={plan_cursor}"), &f.cookie).await, StatusCode::BAD_REQUEST).await;
    assert_eq!(f.reader.calls(), calls);
}

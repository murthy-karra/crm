//! 010f1 HTTP contracts against real retained synthetic source and completed
//! People imports. Workers run explicitly; no provider/network or timing sleeps.
//! TRUST/CONTRACT/BOUNDARY: current authority, exact receipts, scoped evidence,
//! body/page limits and retained review state are observable at the HTTP seam.
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf, sync::Arc};

use axum::{body::Body, http::Request, http::StatusCode, response::Response};
use crm_api::domain::migration::{
    imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
    metadata_worker,
    snapshot_source::Stream,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{
    common::{body_json, get_with_cookie, post_json_with_cookie},
    import_support::{self as support, Book, Fixture},
};

const ROOT: &str = "/api/migrations/fub/metadata-imports";

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

#[sqlx::test]
#[ignore]
async fn metadata_http_committed_receipt_survives_expired_and_missing_readiness(migrator: PgPool) {
    use crm_app::auth::workspace;
    let (mut f, parent) = completed_parent(&migrator, book(None)).await;
    let (id, ready) = ready(&f, parent).await;
    let (second, second_parent) = completed_parent(&migrator, book(None)).await;
    let (second_id, second_ready) = self::ready(&second, second_parent).await;
    let report = ReleaseReport(
        std::env::temp_dir().join(format!("crm-010f1-http-release-{}.json", Uuid::new_v4())),
    );
    let now = chrono::Utc::now();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&migrator)
        .await
        .unwrap();
    let mut evidence = json!({"confirmation_ready":true,"metadata_confirmation_ready":true,"database_name":database,"checked_at":now,"evidence_expires_at":now+chrono::Duration::minutes(5),"candidates":[{"sha256":workspace::artifact_fingerprint().await.unwrap(),"gate_version":workspace::GATE_VERSION,"capabilities":["fub-metadata-import-v1"]}]});
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
    assert_eq!(detail(&f, id).await["release_ready"], true);
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
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()).await,
            StatusCode::ACCEPTED
        )
        .await,
        original
    );
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
            post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()).await,
            StatusCode::ACCEPTED
        )
        .await,
        original
    );
    let mut altered = command.clone();
    altered["workspace_revision"] = json!("999999");
    checked(
        post_json_with_cookie(&f.app, &path, &f.cookie, altered).await,
        StatusCode::CONFLICT,
    )
    .await;
    checked(
        post_json_with_cookie(&f.app, &path, &f.member_cookie, command).await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

fn book(extra: Option<&str>) -> Arc<Book> {
    let mut people = vec![
        json!({"id":101,"firstName":"Synthetic One","stage":"Lead","assignedUserId":3,
            "emails":[{"value":"shared@metadata.synthetic"}],"tags":["Synthetic Tag","Second Tag"],
            "customText":"Preserved text","customChoice":"None"}),
        json!({"id":102,"firstName":"Synthetic Two","stage":"Lead","assignedUserId":3,
            "emails":[{"value":"SHARED@metadata.synthetic"}],"tags":["Synthetic Tag"],
            "customText":"Second value","customChoice":"North"}),
        json!({"id":103,"firstName":"Synthetic Held","stage":"Lead","isTrash":true,
            "tags":["Excluded Person Only"],"customText":"Excluded value"}),
    ];
    if let Some(text) = extra {
        people[0]["customExactEvidence"] = json!(text);
    }
    let source = Arc::new(Book::new(people));
    source.set_records(Stream::CustomFields, vec![
        json!({"id":21,"name":"customText","label":"Synthetic Text","type":"text"}),
        json!({"id":22,"name":"customChoice","label":"Synthetic Choice","type":"dropdown","isRecurring":false,"choices":["None","North"]}),
    ]);
    source
}

async fn checked(response: Response, expected: StatusCode) -> Value {
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.len() <= 512 * 1024, "bounded metadata response");
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, expected, "closed error: {}", value["error"]);
    if expected.is_success() {
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
        for private_key in [
            "credential_ciphertext",
            "credential_nonce",
            "lease_token",
            "patch_ciphertext",
            "provenance_nonce",
        ] {
            assert!(
                !String::from_utf8_lossy(&body).contains(private_key),
                "private storage field escaped"
            );
        }
    }
    value
}

async fn completed_parent(migrator: &PgPool, source: Arc<Book>) -> (Fixture, Uuid) {
    let f = support::fixture_with_book(migrator, source).await;
    let (id, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    support::replan(
        &f,
        id,
        "1",
        &[StagePatch {
            source_key: "4".into(),
            choice: StageChoice::Existing {
                stage_id: f.lead_stage,
            },
        }],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Member { user_id: f.actor },
        }],
    )
    .await;
    support::drain_import(&f).await;
    let ready = imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap();
    let command = json!({"request_id":Uuid::new_v4(),"plan_id":ready["plan"]["id"],"plan_revision":ready["plan"]["revision"],"confirmation_digest":ready["plan"]["confirmation_digest"],"acknowledgments":{"held_count":ready["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}});
    imports::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        serde_json::from_value(command).unwrap(),
        &crm_app::auth::workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    support::drain_import(&f).await;
    assert_eq!(
        imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
            .await
            .unwrap()["state"],
        "completed"
    );
    (f, id)
}

async fn drain(f: &Fixture) {
    for _ in 0..2_000 {
        if !metadata_worker::run_once(&f.pool, &f.key, &f.policy)
            .await
            .unwrap()
        {
            return;
        }
    }
    panic!("metadata fixture exceeded bounded work");
}

async fn detail(f: &Fixture, id: Uuid) -> Value {
    checked(
        get_with_cookie(&f.app, &format!("{ROOT}/{id}"), &f.cookie).await,
        StatusCode::OK,
    )
    .await
}

async fn create(f: &Fixture, parent: Uuid) -> Uuid {
    let response = checked(
        post_json_with_cookie(
            &f.app,
            ROOT,
            &f.cookie,
            json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    Uuid::parse_str(response["import"]["id"].as_str().unwrap()).unwrap()
}

async fn page(f: &Fixture, path: &str) -> Value {
    let v = checked(
        get_with_cookie(&f.app, path, &f.cookie).await,
        StatusCode::OK,
    )
    .await;
    assert!(v["items"].is_array());
    v
}

async fn ready(f: &Fixture, parent: Uuid) -> (Uuid, Value) {
    let id = create(f, parent).await;
    drain(f).await;
    let initial = detail(f, id).await;
    assert_eq!(initial["latest_plan"]["state"], "ready");
    let plan = initial["latest_plan"]["id"].as_str().unwrap();
    let mappings = page(f, &format!("{ROOT}/{id}/plans/{plan}/mappings?limit=50")).await;
    assert!(
        mappings["next_cursor"].is_null(),
        "small fixture fits one mapping page"
    );
    let patches: Vec<_> = mappings["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["qualified"] == true)
        .map(|m| json!({"mapping_id":m["id"],"choice":{"kind":"create_matching"}}))
        .collect();
    assert!(!patches.is_empty());
    checked(post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie, json!({"request_id":Uuid::new_v4(),"expected_plan_revision":initial["latest_plan"]["revision"],"mappings":patches})).await, StatusCode::ACCEPTED).await;
    drain(f).await;
    let result = detail(f, id).await;
    assert_eq!(result["latest_plan"]["state"], "ready");
    assert_eq!(result["actions"]["confirm"], true);
    (id, result)
}

fn confirmation(detail: &Value) -> Value {
    json!({"request_id":Uuid::new_v4(),"plan_id":detail["latest_plan"]["id"],"plan_revision":detail["latest_plan"]["revision"],"confirmation_digest":detail["latest_plan"]["confirmation_digest"],"workspace_revision":detail["workspace_revision"],"acknowledgments":{"held_count":detail["latest_plan"]["counts"]["held_count"],"review_only":true,"remaining_data":true}})
}

async fn native_counts(f: &Fixture) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM person WHERE organization_id=$1),(SELECT count(*) FROM tag WHERE organization_id=$1),(SELECT count(*) FROM person_tag WHERE organization_id=$1),(SELECT count(*) FROM person_custom_field_value WHERE organization_id=$1)").bind(f.org).fetch_one(&f.pool).await.unwrap()
}

#[sqlx::test]
#[ignore]
async fn metadata_http_creation_is_admin_scoped_strict_and_exactly_idempotent(migrator: PgPool) {
    let (f, parent) = completed_parent(&migrator, book(None)).await;
    let foreign = support::fixture_with_book(&migrator, book(None)).await;
    let calls = f.reader.calls();
    let command = json!({"request_id":Uuid::new_v4(),"parent_import_id":parent});
    for (cookie, expected) in [
        ("", StatusCode::UNAUTHORIZED),
        (f.member_cookie.as_str(), StatusCode::FORBIDDEN),
        (foreign.cookie.as_str(), StatusCode::NOT_FOUND),
    ] {
        checked(
            post_json_with_cookie(&f.app, ROOT, cookie, command.clone()).await,
            expected,
        )
        .await;
    }
    let mut forged = command.clone();
    forged["organization_id"] = json!(f.org);
    checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, forged).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let accepted = checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, command.clone()).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, ROOT, &f.cookie, command.clone()).await,
            StatusCode::CREATED
        )
        .await,
        accepted
    );
    let mut altered = command;
    altered["parent_import_id"] = json!(Uuid::new_v4());
    checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, altered).await,
        StatusCode::CONFLICT,
    )
    .await;
    let id = Uuid::parse_str(accepted["import"]["id"].as_str().unwrap()).unwrap();
    checked(
        get_with_cookie(&f.app, &format!("{ROOT}/{id}"), &foreign.cookie).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    drain(&f).await;
    assert_eq!(
        native_counts(&f).await,
        (2, 0, 0, 0),
        "planning does not mutate metadata or add People"
    );
    assert_eq!(f.reader.calls(), calls, "retained-only child preparation");
    let oversized = format!("{{\"extra\":\"{}\"}}", "x".repeat(65_536));
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
    for suffix in [
        "?limit=0",
        "?limit=51",
        "?cursor=invalid",
        "?parent_import_id=invalid",
    ] {
        checked(
            get_with_cookie(&f.app, &format!("{ROOT}{suffix}"), &f.cookie).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
}

#[sqlx::test]
#[ignore]
async fn metadata_http_rejects_uncompleted_parent_and_incomplete_custom_fields(migrator: PgPool) {
    let f = support::fixture_with_book(&migrator, book(None)).await;
    let (parent, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    checked(
        post_json_with_cookie(
            &f.app,
            ROOT,
            &f.cookie,
            json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    let (completed, parent) = completed_parent(&migrator, book(None)).await;
    // Damage a captured stream's exhaustion proof, not a business row. Parent
    // completion alone must not stand in for the child's extra eligibility rule.
    sqlx::query("UPDATE migration_snapshot_stream SET state='paused' WHERE snapshot_id=$1 AND stream='custom_fields'").bind(completed.snapshot).execute(&migrator).await.unwrap();
    checked(
        post_json_with_cookie(
            &completed.app,
            ROOT,
            &completed.cookie,
            json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(native_counts(&completed).await, (2, 0, 0, 0));
}

#[sqlx::test]
#[ignore]
async fn metadata_http_frozen_choices_enforce_patch_bounds_and_cursor_scope(migrator: PgPool) {
    let (f, parent) = completed_parent(&migrator, book(None)).await;
    let id = create(&f, parent).await;
    drain(&f).await;
    let first = detail(&f, id).await;
    let old_plan = first["latest_plan"]["id"].as_str().unwrap();
    let rows = page(
        &f,
        &format!("{ROOT}/{id}/plans/{old_plan}/mappings?limit=1"),
    )
    .await;
    let cursor = rows["next_cursor"].as_str().unwrap();
    let mapping = &rows["items"][0];
    assert_eq!(mapping["choice"]["kind"], "hold");
    let body = json!({"request_id":Uuid::new_v4(),"expected_plan_revision":first["latest_plan"]["revision"],"mappings":[{"mapping_id":mapping["id"],"choice":{"kind":"create_matching"}}]});
    let mut forged = body.clone();
    forged["mappings"][0]["choice"]["label"] = json!("Unapproved replacement");
    checked(
        post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie, forged).await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let mut too_many = body.clone();
    too_many["mappings"] = json!(vec![body["mappings"][0].clone(); 51]);
    checked(
        post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie, too_many).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    checked(
        post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/plans"), &f.cookie, body).await,
        StatusCode::ACCEPTED,
    )
    .await;
    drain(&f).await;
    let second = detail(&f, id).await;
    let new_plan = second["latest_plan"]["id"].as_str().unwrap();
    assert_ne!(old_plan, new_plan);
    checked(
        get_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/plans/{new_plan}/mappings?limit=1&cursor={cursor}"),
            &f.cookie,
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    checked(
        get_with_cookie(
            &f.app,
            &format!("{ROOT}/{id}/plans/{new_plan}/records?limit=1&cursor={cursor}"),
            &f.cookie,
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    let mut stale = confirmation(&second);
    stale["plan_revision"] = first["latest_plan"]["revision"].clone();
    checked(
        post_json_with_cookie(&f.app, &format!("{ROOT}/{id}/confirm"), &f.cookie, stale).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(native_counts(&f).await, (2, 0, 0, 0));
}

#[sqlx::test]
#[ignore]
async fn metadata_http_confirmation_replays_and_preserves_parent_and_review_hold(migrator: PgPool) {
    let (f, parent) = completed_parent(&migrator, book(None)).await;
    let before = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    let calls = f.reader.calls();
    let (id, ready) = ready(&f, parent).await;
    let command = confirmation(&ready);
    let mut unchecked = command.clone();
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
    let path = format!("{ROOT}/{id}/confirm");
    let (left, right) = tokio::join!(
        post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()),
        post_json_with_cookie(&f.app, &path, &f.cookie, command.clone())
    );
    let receipt = checked(left, StatusCode::ACCEPTED).await;
    assert_eq!(checked(right, StatusCode::ACCEPTED).await, receipt);
    drain(&f).await;
    assert_eq!(detail(&f, id).await["state"], "completed");
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()).await,
            StatusCode::ACCEPTED
        )
        .await,
        receipt
    );
    let mut altered = command;
    altered["acknowledgments"]["held_count"] = json!("999999");
    checked(
        post_json_with_cookie(&f.app, &path, &f.cookie, altered).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(native_counts(&f).await, (2, 2, 3, 4));
    let literal: String = sqlx::query_scalar("SELECT o.label FROM person_custom_field_value v JOIN custom_field_option o ON o.organization_id=v.organization_id AND o.id=v.option_id JOIN migration_import_identity i ON i.organization_id=v.organization_id AND i.target_id=v.person_id WHERE i.organization_id=$1 AND i.family='people' AND i.source_id='101'").bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        literal, "None",
        "declared literal is an ordinary mapped choice"
    );
    let after = imports::detail(&f.pool, &f.key, &f.ctx, parent, &f.policy)
        .await
        .unwrap();
    for key in [
        "state",
        "confirmed_plan_id",
        "counts",
        "workspace",
        "capture_sequence",
        "retained_bytes",
        "reserved_bytes",
    ] {
        assert_eq!(before[key], after[key], "parent {key} is immutable");
    }
    assert_eq!(f.reader.calls(), calls);
    let results = page(&f, &format!("{ROOT}/{id}/results?limit=50")).await;
    let person = results["items"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|v| v["person_id"].as_str())
        .unwrap();
    let provenance = format!("/api/people/{person}/metadata-import-provenance");
    assert!(!page(&f, &provenance).await["items"]
        .as_array()
        .unwrap()
        .is_empty());
    for path in [
        ROOT.to_owned(),
        format!("{ROOT}/{id}"),
        format!("{ROOT}/{id}/results"),
        provenance,
    ] {
        checked(
            get_with_cookie(&f.app, &path, &f.member_cookie).await,
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    assert_eq!(
        get_with_cookie(&f.app, "/api/people", &f.member_cookie)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        post_json_with_cookie(
            &f.app,
            "/api/tags",
            &f.cookie,
            json!({"name":"Must remain blocked"})
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
#[ignore]
async fn metadata_http_cancel_is_terminal_replayable_and_does_not_release_workspace(
    migrator: PgPool,
) {
    let (f, parent) = completed_parent(&migrator, book(None)).await;
    let id = create(&f, parent).await;
    drain(&f).await;
    let command = json!({"request_id":Uuid::new_v4()});
    let path = format!("{ROOT}/{id}/cancel");
    let receipt = checked(
        post_json_with_cookie(&f.app, &path, &f.cookie, command.clone()).await,
        StatusCode::ACCEPTED,
    )
    .await;
    assert_eq!(
        checked(
            post_json_with_cookie(&f.app, &path, &f.cookie, command).await,
            StatusCode::ACCEPTED
        )
        .await,
        receipt
    );
    let cancelled = detail(&f, id).await;
    assert_eq!(cancelled["state"], "cancelled");
    assert_eq!(cancelled["reserved_bytes"], "0");
    assert_eq!(cancelled["actions"]["retry"], false);
    checked(
        post_json_with_cookie(
            &f.app,
            ROOT,
            &f.cookie,
            json!({"request_id":Uuid::new_v4(),"parent_import_id":parent}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
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
    assert!(!metadata_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    assert_eq!(native_counts(&f).await, (2, 0, 0, 0));
    assert_eq!(
        body_json(get_with_cookie(&f.app, "/api/me", &f.cookie).await).await["organization"]
            ["workspace_mode"],
        "migration_review"
    );
}

#[sqlx::test]
#[ignore]
async fn metadata_http_full_evidence_segments_bind_person_field_and_revision(migrator: PgPool) {
    let exact = "Synthetic José 🏡 <script>text only</script> \\\"quoted\\\"\n".repeat(40_000);
    assert!(exact.len() > 2 * 1024 * 1024);
    let (f, parent) = completed_parent(&migrator, book(Some(&exact))).await;
    let id = create(&f, parent).await;
    drain(&f).await;
    let detail = detail(&f, id).await;
    let plan = detail["latest_plan"]["id"].as_str().unwrap();
    let rows = page(&f, &format!("{ROOT}/{id}/plans/{plan}/records?limit=50")).await;
    let records = rows["items"].as_array().unwrap();
    let row = records.iter().find(|v| v["source_id"] == "101").unwrap();
    assert_eq!(row["source"]["abbreviated"], true);
    let record = row["id"].as_str().unwrap();
    let other = records.iter().find(|v| v["source_id"] == "102").unwrap()["id"]
        .as_str()
        .unwrap();
    let base = format!("{ROOT}/{id}/plans/{plan}/records/{record}/fields/source.all");
    let mut all = String::new();
    let mut cursor: Option<String> = None;
    let mut total = None;
    let mut first_cursor = None;
    for _ in 0..300 {
        let path = format!(
            "{base}?limit=65536{}",
            cursor
                .as_ref()
                .map(|v| format!("&cursor={v}"))
                .unwrap_or_default()
        );
        let segment = checked(
            get_with_cookie(&f.app, &path, &f.cookie).await,
            StatusCode::OK,
        )
        .await;
        assert_eq!(segment["offset_bytes"], all.len().to_string());
        let text = segment["text"].as_str().unwrap();
        assert!(!text.is_empty());
        all.push_str(text);
        total = segment["full_utf8_bytes"].as_str().map(str::to_owned);
        cursor = segment["next_cursor"].as_str().map(str::to_owned);
        if first_cursor.is_none() {
            first_cursor = cursor.clone();
        }
        if cursor.is_none() {
            assert_eq!(segment["complete"], true);
            break;
        }
    }
    assert!(cursor.is_none(), "all bounded segments terminate");
    assert_eq!(total.unwrap(), all.len().to_string());
    let decoded: Value = serde_json::from_str(&all).unwrap();
    assert!(decoded.to_string().contains("customExactEvidence"));
    // Exact original string remains present after peeling the canonical-evidence
    // JSON string layer. No rendered HTML or clipping stands in for source bytes.
    fn contains_exact(value: &Value, exact: &str) -> bool {
        match value {
            Value::String(v) => {
                v == exact
                    || serde_json::from_str::<Value>(v).is_ok_and(|v| contains_exact(&v, exact))
            }
            Value::Array(v) => v.iter().any(|v| contains_exact(v, exact)),
            Value::Object(v) => v.values().any(|v| contains_exact(v, exact)),
            _ => false,
        }
    }
    assert!(contains_exact(&decoded, &exact));
    let cursor = first_cursor.unwrap();
    for path in [
        format!("{ROOT}/{id}/plans/{plan}/records/{other}/fields/source.all?cursor={cursor}"),
        format!("{ROOT}/{id}/plans/{plan}/records/{record}/fields/operations.all?cursor={cursor}"),
    ] {
        checked(
            get_with_cookie(&f.app, &path, &f.cookie).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    checked(
        get_with_cookie(&f.app, &base, &f.member_cookie).await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

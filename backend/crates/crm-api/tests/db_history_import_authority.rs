//! T1 TRUST/CONTRACT: full-router authority and rejected-body boundaries.
//! All source material is synthetic; membership edits are migrator controls.
use crate::{
    common::{
        body_json, create_platform_admin, get_with_cookie, login_cookie, post_json_with_cookie,
    },
    db_history_capture_support as capture, db_history_import_support as imports,
    import_support::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const ROOT: &str = "/api/migrations/fub/history-imports";

async fn closed(response: Response, status: StatusCode) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let code = match status {
        StatusCode::UNAUTHORIZED => "unauthenticated",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::BAD_REQUEST => "malformed_request",
        _ => panic!("unexpected test error status"),
    };
    // Exact closed envelopes cannot contain retained bodies, IDs or input text.
    assert_eq!(body_json(response).await, json!({"error":code}));
}

async fn prepare_body(f: &Fixture, parent: Uuid, source: Uuid) -> Value {
    let captured = capture::ready(f, source).await;
    let workspace: i64 =
        sqlx::query_scalar("SELECT workspace_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    json!({"request_id":Uuid::new_v4(),"parent_import_id":parent,"capture_id":source,
        "expected_capture_revision":captured["revision"],
        "expected_workspace_revision":workspace.to_string(),
        "expected_policy_revision":f.policy.revision()})
}

fn writes(id: Uuid, prepare: Value, detail: &Value) -> Vec<(String, Value)> {
    let action = json!({"request_id":Uuid::new_v4(),"expected_revision":detail["revision"]});
    vec![
        (ROOT.into(), prepare),
        (
            format!("{ROOT}/{id}/confirm"),
            serde_json::to_value(imports::confirmation(detail)).unwrap(),
        ),
        (
            format!("{ROOT}/{id}/resume"),
            json!({"request_id":Uuid::new_v4(),"expected_revision":detail["revision"],"expected_policy_revision":detail["policy_revision"]}),
        ),
        (format!("{ROOT}/{id}/cancel"), action),
        (
            format!("{ROOT}/{id}/budget"),
            json!({"request_id":Uuid::new_v4(),"expected_revision":detail["revision"],
            "expected_run_budget_revision":detail["run_budget_revision"],"expected_org_budget_revision":detail["org_budget_revision"],
            "expected_policy_revision":detail["policy_revision"],"run_byte_limit":detail["run_byte_limit"],"org_byte_limit":detail["org_byte_limit"]}),
        ),
    ]
}

async fn unchanged_state(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('runs',(SELECT md5(COALESCE(jsonb_agg(to_jsonb(r) ORDER BY r.id),'[]'::jsonb)::text) FROM migration_history_import_run r WHERE organization_id=$1),'receipts',(SELECT count(*) FROM migration_history_import_receipt WHERE organization_id=$1),'anchors',(SELECT count(*) FROM migration_history_import_anchor WHERE organization_id=$1),'storage',(SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1))")
        .bind(f.org).fetch_one(&f.pool).await.unwrap()
}

#[sqlx::test]
#[ignore]
async fn full_router_import_authority_covers_every_read_and_write_without_source_io(pool: PgPool) {
    let (f, parent, source, book) = imports::fixture(&pool).await;
    let id = imports::ready(&f, parent, source).await;
    let foreign =
        crate::import_support::fixture(&pool, crate::import_support::default_people()).await;
    let email = format!("history-platform-{}@synthetic.test", Uuid::new_v4());
    let password = "synthetic history authority platform password";
    create_platform_admin(&pool, &email, "Synthetic platform operator", password).await;
    let platform_cookie = login_cookie(&f.app, &email, password).await;
    let me = body_json(get_with_cookie(&f.app, "/api/me", &platform_cookie).await).await;
    assert_eq!(me["platform_admin"], true);
    assert!(me["organization"].is_null());
    let detail = imports::detail(&f, id).await;
    let posts = writes(id, prepare_body(&f, parent, source).await, &detail);
    let reads = [
        format!("{ROOT}?parent_import_id={parent}"),
        format!("{ROOT}/{id}"),
        format!("{ROOT}/{id}/records"),
        format!("{ROOT}/{id}/results"),
    ];
    let before = unchanged_state(&f).await;
    let source_calls = (f.reader.calls(), book.count(), foreign.reader.calls());
    for path in &reads {
        let response = get_with_cookie(&f.app, path, &f.cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = body_json(response).await.to_string();
        for sentinel in [
            "IMPORT_BODY_SENTINEL",
            "IMPORT_PHONE_SENTINEL",
            "IMPORT_TEXT_SENTINEL",
        ] {
            assert!(!body.contains(sentinel));
        }
    }
    for (cookie, status) in [
        ("", StatusCode::UNAUTHORIZED),
        (f.member_cookie.as_str(), StatusCode::FORBIDDEN),
        (platform_cookie.as_str(), StatusCode::UNAUTHORIZED),
        (foreign.cookie.as_str(), StatusCode::NOT_FOUND),
    ] {
        for path in &reads {
            closed(get_with_cookie(&f.app, path, cookie).await, status).await;
        }
        for (path, body) in &posts {
            closed(
                post_json_with_cookie(&f.app, path, cookie, body.clone()).await,
                status,
            )
            .await;
        }
    }
    // An unfiltered foreign administrator list is a legitimate empty own-Org read.
    let own = get_with_cookie(&f.app, ROOT, &foreign.cookie).await;
    assert_eq!(own.status(), StatusCode::OK);
    assert_eq!(own.headers()["cache-control"], "no-store");
    assert_eq!(
        body_json(own).await,
        json!({"imports":[],"next_cursor":null})
    );
    assert_eq!(unchanged_state(&f).await, before);
    assert_eq!(
        (f.reader.calls(), book.count(), foreign.reader.calls()),
        source_calls
    );
}

#[sqlx::test]
#[ignore]
async fn malformed_requests_and_current_revocation_cannot_replay_a_confirmed_receipt(pool: PgPool) {
    let (f, parent, source, book) = imports::fixture(&pool).await;
    let source_calls = (f.reader.calls(), book.count());
    let prepare = prepare_body(&f, parent, source).await;
    let response = post_json_with_cookie(&f.app, ROOT, &f.cookie, prepare.clone()).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let receipt = body_json(response).await;
    let id = Uuid::parse_str(receipt["import_id"].as_str().unwrap()).unwrap();
    imports::drain(&f).await;
    let detail = imports::detail(&f, id).await;
    assert_eq!(detail["state"], "ready");
    let before = unchanged_state(&f).await;
    for (path, body) in writes(id, prepare, &detail) {
        let mut unknown = body.clone();
        unknown["untrusted_extra"] = json!("MUST_NOT_APPEAR_IN_ERROR");
        closed(
            post_json_with_cookie(&f.app, &path, &f.cookie, unknown).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
        // Valid JSON with only extra whitespace: only the wire-byte cap rejects it.
        let oversized = format!("{}{}", " ".repeat(8193), body);
        let response = f
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&path)
                    .header("content-type", "application/json")
                    .header("cookie", &f.cookie)
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();
        closed(response, StatusCode::BAD_REQUEST).await;
    }
    assert_eq!(unchanged_state(&f).await, before);
    let confirm = serde_json::to_value(imports::confirmation(&detail)).unwrap();
    let path = format!("{ROOT}/{id}/confirm");
    let response = post_json_with_cookie(&f.app, &path, &f.cookie, confirm.clone()).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let confirmed = body_json(response).await;
    let replay = post_json_with_cookie(&f.app, &path, &f.cookie, confirm.clone()).await;
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    assert_eq!(replay.headers()["cache-control"], "no-store");
    assert_eq!(body_json(replay).await, confirmed);
    // Keep a real active administrator while revoking this exact session actor.
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap();
    let before = unchanged_state(&f).await;
    closed(
        post_json_with_cookie(&f.app, &path, &f.cookie, confirm).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(unchanged_state(&f).await, before);
    assert_eq!((f.reader.calls(), book.count()), source_calls);
}

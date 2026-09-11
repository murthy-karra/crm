//! Independent HTTP acceptance tests for the approved 010b contract.
//! Only synthetic identities are used; no worker or real FUB client runs here.
use axum::{http::StatusCode, response::Response, Router};
use crm_api::{
    config::Config,
    domain::{
        admin::{MembershipStatus, Role},
        migration::reader::{FubReader, Identity, Probe, ProbeResult, ReaderError},
    },
    realtime::Publisher,
    state::AppState,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use uuid::Uuid;

use crate::common::{body_json, get_with_cookie, login_cookie, post_json_with_cookie};

const ROOT: &str = "/api/migrations/fub/snapshots";
const PW: &str = "synthetic snapshot fixture password";

#[derive(Default)]
struct IdentityOnlyReader {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl FubReader for IdentityOnlyReader {
    async fn identity(&self, _: &str) -> Result<(Identity, Vec<u8>), ReaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((
            Identity {
                account_id: 171,
                account_domain: Some("snapshot.synthetic.test".into()),
                user_id: Some(17),
                display_name: Some("Synthetic snapshot account".into()),
            },
            br#"{"account":{"id":171,"domain":"snapshot.synthetic.test"},"user":{"id":17}}"#
                .to_vec(),
        ))
    }

    async fn probe(&self, _: &str, _: Probe) -> Result<ProbeResult, ReaderError> {
        panic!("HTTP preparation must not execute source probes")
    }
}

struct Fixture {
    app: Router,
    cookie: String,
    org: Uuid,
    actor: Uuid,
    connection: Value,
    reader: Arc<IdentityOnlyReader>,
}

async fn add_admin(migrator: &PgPool, org: Uuid, email: &str) -> Uuid {
    let user = crate::common::create_user(migrator, email, "Synthetic administrator", PW).await;
    crate::common::add_membership_with(migrator, org, user, Role::Admin, MembershipStatus::Active)
        .await;
    user
}

async fn fixture(migrator: &PgPool, config: Config) -> Fixture {
    let org = crate::common::create_org(migrator, "Snapshot HTTP synthetic").await;
    let actor = add_admin(migrator, org, "snapshot-admin@synthetic.test").await;
    let reader = Arc::new(IdentityOnlyReader::default());
    let pool = crate::common::connect_as_app(migrator).await;
    let app = crm_api::build_app(
        AppState::for_tests(pool, &config, Publisher::recording())
            .with_migration_reader(reader.clone()),
    );
    let cookie = login_cookie(&app, "snapshot-admin@synthetic.test", PW).await;
    let connection = checked(
        post_json_with_cookie(
            &app,
            "/api/migrations/fub/connections",
            &cookie,
            json!({"request_id": Uuid::new_v4(), "api_key": "synthetic-http-only-value"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await["connection"]
        .clone();
    Fixture {
        app,
        cookie,
        org,
        actor,
        connection,
        reader,
    }
}

async fn checked(response: Response, expected: StatusCode) -> Value {
    assert_eq!(response.status(), expected);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value = body_json(response).await;
    let rendered = value.to_string();
    for forbidden in [
        "synthetic-http-only-value",
        "credential_ciphertext",
        "credential_nonce",
        "cursor_ciphertext",
    ] {
        assert!(!rendered.contains(forbidden));
    }
    value
}

fn proposal_input(f: &Fixture, request_id: Uuid) -> Value {
    json!({
        "request_id": request_id,
        "connection_id": f.connection["id"],
        "expected_revision": f.connection["revision"],
    })
}

async fn propose(f: &Fixture) -> Value {
    checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, proposal_input(f, Uuid::new_v4())).await,
        StatusCode::CREATED,
    )
    .await
}

fn path(snapshot: &Value, suffix: &str) -> String {
    format!("{ROOT}/{}{suffix}", snapshot["id"].as_str().unwrap())
}

async fn confirm(f: &Fixture, snapshot: &Value) -> Value {
    checked(
        post_json_with_cookie(
            &f.app,
            &path(snapshot, "/confirm"),
            &f.cookie,
            json!({"request_id": Uuid::new_v4()}),
        )
        .await,
        StatusCode::ACCEPTED,
    )
    .await["snapshot"]
        .clone()
}

#[sqlx::test]
#[ignore]
async fn proposal_and_concurrent_confirmation_replay_without_source_or_business_writes(
    migrator: PgPool,
) {
    let f = fixture(&migrator, crate::common::test_config()).await;
    let request = Uuid::new_v4();
    let input = proposal_input(&f, request);
    let first = checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, input.clone()).await,
        StatusCode::CREATED,
    )
    .await;
    let replay = checked(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, input).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(first, replay);
    assert_eq!(first["snapshot"]["state"], "proposed");
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 1);
    assert_eq!(first["proposal"]["families"].as_array().unwrap().len(), 6);

    let mut changed = proposal_input(&f, request);
    changed["expected_revision"] = json!(999);
    assert_eq!(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, changed)
            .await
            .status(),
        StatusCode::CONFLICT,
    );
    let uri = path(&first["snapshot"], "/confirm");
    let body = json!({"request_id": Uuid::new_v4()});
    let (left, right) = tokio::join!(
        post_json_with_cookie(&f.app, &uri, &f.cookie, body.clone()),
        post_json_with_cookie(&f.app, &uri, &f.cookie, body),
    );
    let left = checked(left, StatusCode::ACCEPTED).await;
    assert_eq!(left, checked(right, StatusCode::ACCEPTED).await);
    assert_eq!(left["snapshot"]["state"], "queued");
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 1);
    let listed = checked(
        get_with_cookie(&f.app, ROOT, &f.cookie).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(listed["active_snapshot_id"], first["snapshot"]["id"]);
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM person WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&migrator)
        .await
        .unwrap();
    assert_eq!(people, 0);
    let old_summary =
        body_json(get_with_cookie(&f.app, "/api/migrations/fub/", &f.cookie).await).await;
    let mut keys: Vec<_> = old_summary
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "active_assessment",
            "connection",
            "latest_assessment",
            "latest_report"
        ]
    );
}

#[sqlx::test]
#[ignore]
async fn http_rejects_forged_scope_unknown_fields_and_invalid_page_inputs(migrator: PgPool) {
    let f = fixture(&migrator, crate::common::test_config()).await;
    assert_eq!(
        get_with_cookie(&f.app, ROOT, "").await.status(),
        StatusCode::UNAUTHORIZED
    );
    let mut forged = proposal_input(&f, Uuid::new_v4());
    forged["organization_id"] = json!(Uuid::new_v4());
    assert_eq!(
        post_json_with_cookie(&f.app, ROOT, &f.cookie, forged)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let original = propose(&f).await;
    let snapshot = &original["snapshot"];
    let foreign_org = crate::common::create_org(&migrator, "Foreign snapshot organization").await;
    add_admin(&migrator, foreign_org, "foreign-snapshot@synthetic.test").await;
    let foreign_cookie = login_cookie(&f.app, "foreign-snapshot@synthetic.test", PW).await;
    for suffix in ["", "/previews/00000000-0000-4000-8000-000000000001"] {
        assert_eq!(
            get_with_cookie(&f.app, &path(snapshot, suffix), &foreign_cookie)
                .await
                .status(),
            StatusCode::NOT_FOUND,
        );
    }
    assert_eq!(
        post_json_with_cookie(
            &f.app,
            &path(snapshot, "/confirm"),
            &foreign_cookie,
            json!({"request_id": Uuid::new_v4()})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND,
    );
    for query in [
        "?cursor=forged",
        "?limit=21",
        "?limit=0",
        "?organization_id=forged",
    ] {
        assert_eq!(
            get_with_cookie(&f.app, &format!("{ROOT}{query}"), &f.cookie)
                .await
                .status(),
            StatusCode::BAD_REQUEST,
        );
    }
    let member =
        crate::common::create_user(&migrator, "snapshot-member@synthetic.test", "Member", PW).await;
    crate::common::add_membership(&migrator, f.org, member).await;
    let member_cookie = login_cookie(&f.app, "snapshot-member@synthetic.test", PW).await;
    assert_eq!(
        get_with_cookie(&f.app, &path(snapshot, ""), &member_cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_json_with_cookie(
            &f.app,
            ROOT,
            &member_cookie,
            proposal_input(&f, Uuid::new_v4())
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    crate::common::create_platform_admin(
        &migrator,
        "snapshot-member@synthetic.test",
        "Synthetic platform administrator",
        PW,
    )
    .await;
    assert_eq!(
        get_with_cookie(&f.app, &path(snapshot, ""), &member_cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "platform administration does not grant Organization source access",
    );
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test]
#[ignore]
async fn current_admin_keeps_retained_access_after_initiator_revocation_and_disconnect(
    migrator: PgPool,
) {
    let f = fixture(&migrator, crate::common::test_config()).await;
    let proposed = propose(&f).await;
    let snapshot = confirm(&f, &proposed["snapshot"]).await;
    add_admin(&migrator, f.org, "replacement-admin@synthetic.test").await;
    let other = login_cookie(&f.app, "replacement-admin@synthetic.test", PW).await;
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2")
        .bind(f.org).bind(f.actor).execute(&migrator).await.unwrap();
    let revoked_status = get_with_cookie(&f.app, &path(&snapshot, ""), &f.cookie)
        .await
        .status();
    assert!(matches!(
        revoked_status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ));
    checked(
        get_with_cookie(&f.app, &path(&snapshot, ""), &other).await,
        StatusCode::OK,
    )
    .await;
    let connection_path = format!(
        "/api/migrations/fub/connections/{}",
        f.connection["id"].as_str().unwrap()
    );
    assert_eq!(
        crate::common::delete_with_cookie(&f.app, &connection_path, &other)
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let retained = checked(
        get_with_cookie(&f.app, &path(&snapshot, ""), &other).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(retained["snapshot"]["id"], snapshot["id"]);
    assert!(!retained["snapshot"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("retry")));
    let cancelled = checked(
        post_json_with_cookie(&f.app, &path(&snapshot, "/cancel"), &other, json!({})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(cancelled["snapshot"]["state"], "cancelled");
    let repeated = checked(
        post_json_with_cookie(&f.app, &path(&snapshot, "/cancel"), &other, json!({})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(repeated["snapshot"]["state"], "cancelled");
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test]
#[ignore]
async fn budget_revision_receipts_preserve_original_limits_and_never_resume_work(migrator: PgPool) {
    let mut config = crate::common::test_config();
    config.snapshot_policy.run_ceiling_bytes = 4 * 1024 * 1024 * 1024;
    config.snapshot_policy.org_ceiling_bytes = 8 * 1024 * 1024 * 1024;
    let f = fixture(&migrator, config).await;
    let proposed = propose(&f).await;
    let snapshot = confirm(&f, &proposed["snapshot"]).await;
    checked(
        post_json_with_cookie(&f.app, &path(&snapshot, "/cancel"), &f.cookie, json!({})).await,
        StatusCode::OK,
    )
    .await;
    let input = json!({
        "request_id": Uuid::new_v4(),
        "expected_run_budget_revision": snapshot["run_budget_revision"],
        "expected_org_budget_revision": snapshot["org_budget_revision"],
        "expected_policy_revision": snapshot["policy_revision"],
        "run_byte_limit": "3221225472",
        "org_byte_limit": "6442450944",
    });
    let uri = path(&snapshot, "/budget");
    let increased = checked(
        post_json_with_cookie(&f.app, &uri, &f.cookie, input.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(increased["budget"]["approved_by_user_id"], json!(f.actor));
    assert_eq!(
        increased["budget"]["old_run_budget_revision"],
        snapshot["run_budget_revision"]
    );
    assert_eq!(
        increased["budget"]["old_org_budget_revision"],
        snapshot["org_budget_revision"]
    );
    assert_eq!(
        increased["budget"]["run_budget_revision"],
        increased["snapshot"]["run_budget_revision"]
    );
    assert_eq!(
        increased["budget"]["org_budget_revision"],
        increased["snapshot"]["org_budget_revision"]
    );
    assert_eq!(
        increased["budget"]["old_run_policy_revision"],
        snapshot["run_budget_policy_revision"]
    );
    assert_eq!(
        increased["budget"]["old_org_policy_revision"],
        snapshot["org_budget_policy_revision"]
    );
    assert_eq!(
        increased["budget"]["policy_revision"],
        snapshot["policy_revision"]
    );
    assert_eq!(increased["snapshot"]["state"], "cancelled");
    assert_eq!(increased["snapshot"]["run_byte_limit"], "3221225472");
    assert_eq!(increased["snapshot"]["org_byte_limit"], "6442450944");
    assert_eq!(
        increased["snapshot"]["original_run_byte_limit"],
        snapshot["original_run_byte_limit"]
    );
    let replay = checked(
        post_json_with_cookie(&f.app, &uri, &f.cookie, input.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(increased, replay);
    add_admin(&migrator, f.org, "snapshot-budget-other@synthetic.test").await;
    let other = login_cookie(&f.app, "snapshot-budget-other@synthetic.test", PW).await;
    assert_eq!(
        post_json_with_cookie(&f.app, &uri, &other, input.clone())
            .await
            .status(),
        StatusCode::CONFLICT
    );

    let mut stale = input.clone();
    stale["request_id"] = json!(Uuid::new_v4());
    assert_eq!(
        post_json_with_cookie(&f.app, &uri, &f.cookie, stale)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let mut changed_replay = input;
    changed_replay["run_byte_limit"] = json!("3758096384");
    assert_eq!(
        post_json_with_cookie(&f.app, &uri, &f.cookie, changed_replay)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let new = &increased["snapshot"];
    let mut over_ceiling = json!({
        "request_id": Uuid::new_v4(),
        "expected_run_budget_revision": new["run_budget_revision"],
        "expected_org_budget_revision": new["org_budget_revision"],
        "expected_policy_revision": new["policy_revision"],
        "run_byte_limit": "4294967297", "org_byte_limit": "6442450944",
    });
    assert_eq!(
        post_json_with_cookie(&f.app, &uri, &f.cookie, over_ceiling.clone())
            .await
            .status(),
        StatusCode::CONFLICT
    );
    over_ceiling["run_byte_limit"] = json!(4_294_967_296_u64);
    assert_eq!(
        post_json_with_cookie(&f.app, &uri, &f.cookie, over_ceiling)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let final_state = checked(
        get_with_cookie(&f.app, &path(&snapshot, ""), &f.cookie).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        final_state["snapshot"]["run_byte_limit"],
        new["run_byte_limit"]
    );
    assert_eq!(final_state["snapshot"]["state"], "cancelled");
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), 1);
}

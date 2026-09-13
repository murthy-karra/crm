//! Mobile001 real PostgreSQL/router contract and failure-path proof.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use crm_api::{
    auth::AuthContext,
    domain::{
        admin::{MembershipStatus, Role},
        envelope::CommandContext,
        mobile, task,
    },
    ids::{OrganizationId, PersonId, TaskId, UserId},
    realtime::Publisher,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
const PW: &str = "mobile-fixture-password-12345";
struct Fixture {
    org: Uuid,
    actor: Uuid,
    other: Uuid,
    person: Uuid,
    app: PgPool,
    router: Router,
    cookie: String,
    context: Uuid,
    install: Uuid,
    bootstrap: Value,
}
fn record(name: &str, value: &Value) {
    if let Ok(directory) = std::env::var("MOBILE_FIXTURE_DIR") {
        let path = std::path::Path::new(&directory);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join(format!("{name}.json")),
            serde_json::to_string_pretty(value).unwrap() + "\n",
        )
        .unwrap();
    }
}
async fn request(
    router: &Router,
    cookie: &str,
    context: Option<Uuid>,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("cookie", cookie);
    if let Some(context) = context {
        builder = builder.header("x-mobile-context", context.to_string());
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn id(value: &Value, key: &str) -> Uuid {
    value[key].as_str().unwrap().parse().unwrap()
}
async fn fixture(pool: &PgPool) -> Fixture {
    let (org, actor) = crate::common::create_org_with_stages_and_member(
        pool,
        "Mobile Fixture",
        "mobile@fixture.test",
        "Mobile Agent",
        PW,
    )
    .await;
    let other = crate::common::create_user(pool, "second@fixture.test", "Other Agent", PW).await;
    crate::common::add_membership_with(pool, org, other, Role::Member, MembershipStatus::Active)
        .await;
    let app = crate::common::connect_as_app(pool).await;
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(org)
    .fetch_one(&app)
    .await
    .unwrap();
    let person:Uuid=sqlx::query_scalar("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) VALUES($1,'Synthetic Person',$2,$3) RETURNING id").bind(org).bind(stage).bind(actor).fetch_one(&app).await.unwrap();
    let router = crate::common::build_router(pool).await;
    let cookie = crate::common::login_cookie(&router, "mobile@fixture.test", PW).await;
    let install = Uuid::new_v4();
    let (status, bootstrap) = request(
        &router,
        &cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":install}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bootstrap: {bootstrap}");
    let context = id(&bootstrap, "context_id");
    assert_eq!(bootstrap["actor_user_id"], actor.to_string());
    assert_eq!(bootstrap["organization_id"], org.to_string());
    let authorized: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(bootstrap["authorized_at"].clone()).unwrap();
    let expires: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(bootstrap["offline_access_expires_at"].clone()).unwrap();
    assert_eq!(expires - authorized, chrono::Duration::days(7));
    Fixture {
        org,
        actor,
        other,
        person,
        app,
        router,
        cookie,
        context,
        install,
        bootstrap,
    }
}
impl Fixture {
    fn auth(&self) -> AuthContext {
        AuthContext {
            actor_user_id: UserId(self.actor),
            actor_email: "mobile@fixture.test".into(),
            actor_display_name: "Mobile Agent".into(),
            active_organization_id: OrganizationId(self.org),
            active_organization_name: "Mobile Fixture".into(),
            role: Role::Member,
        }
    }
    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        request(
            &self.router,
            &self.cookie,
            Some(self.context),
            "POST",
            path,
            body,
        )
        .await
    }
    fn operation(&self, kind: &str, payload: Value) -> Value {
        json!({"context_id":self.context,"operation_id":Uuid::new_v4(),"kind":kind,"device_recorded_at":"2026-09-12T20:00:00Z","payload":payload})
    }
    async fn gen(&self) -> Value {
        let (status,value)=self.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":self.install,"pinned_person_ids":[]})).await;
        assert_eq!(status, StatusCode::OK, "generation: {value}");
        value
    }
}
#[sqlx::test]
#[ignore]
async fn atomic_replay_conflict_dependency_and_old_web_commands(pool: PgPool) {
    let f = fixture(&pool).await;
    let note = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"  Field note\r\nSaved  "}),
    );
    let (left, right) = tokio::join!(
        f.post("/api/mobile/v1/operations", note.clone()),
        f.post("/api/mobile/v1/operations", note.clone())
    );
    assert_eq!(left.0, StatusCode::OK, "left: {}", left.1);
    assert_eq!(right.0, StatusCode::OK, "right: {}", right.1);
    assert_eq!(left.1["resource_id"], right.1["resource_id"]);
    assert_ne!(left.1["replayed"], right.1["replayed"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    let mut normalized = note.clone();
    normalized["payload"]["body"] = json!("Field note\nSaved");
    let (status, replay) = f.post("/api/mobile/v1/operations", normalized).await;
    record("operation_replay", &replay);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    let mut mismatch = note.clone();
    mismatch["payload"]["body"] = json!("Different proposal");
    let (status, error) = f.post("/api/mobile/v1/operations", mismatch).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"], "operation_payload_mismatch");
    record("payload_mismatch", &error);
    let create=f.operation("create_task",json!({"person_id":f.person,"title":"Visit after outage","kind":"follow_up","due_at":null,"assignee_user_id":null}));
    let complete = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"created_by_operation_id":create["operation_id"]}}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", complete.clone())
            .await
            .1["error"],
        "dependency_pending"
    );
    let (status, created) = f.post("/api/mobile/v1/operations", create.clone()).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["committed_revision"], "1");
    record("create_task_request", &create);
    record("create_task_receipt", &created);
    let (status, completed) = f.post("/api/mobile/v1/operations", complete.clone()).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["committed_revision"], "2");
    record("complete_dependency_request", &complete);
    record("complete_task_receipt", &completed);
    let task_id = id(&created, "resource_id");
    task::reopen_task(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&f.auth()),
        task::ReopenTask {
            person_id: PersonId(f.person),
            task_id: TaskId(task_id),
        },
    )
    .await
    .unwrap();
    let (status, replay) = f.post("/api/mobile/v1/operations", complete).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    let still_open: bool = sqlx::query_scalar("SELECT completed_at IS NULL FROM task WHERE id=$1")
        .bind(task_id)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert!(still_open, "replay must not complete a reopened task");
    let stale = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"task_id":task_id,"expected_revision":"1"}}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", stale).await.1["error"],
        "revision_conflict"
    );
    // Database failure after business write but before receipt must roll back both.
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic receipt failure'; END $$; CREATE TRIGGER mobile_fixture_failure BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile_fixture_fail_receipt();").execute(&pool).await.unwrap();
    let failed = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"Must stay queued"}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", failed.clone()).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER mobile_fixture_failure ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", failed).await.0,
        StatusCode::OK
    );
    // Seven-day lock protects old queue identity; same-identity bootstrap renews.
    sqlx::query(
        "UPDATE mobile_context SET offline_access_expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(f.context)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", note.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (_, renewed) = request(
        &f.router,
        &f.cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":f.install}),
    )
    .await;
    assert_eq!(renewed["context_id"], f.context.to_string());
    record("bootstrap", &renewed);
    record("add_note_request", &note);
    assert_eq!(
        f.post("/api/mobile/v1/operations", note).await.1["replayed"],
        true
    );
    // Payloads never enter durable receipts; only keyed digests and result IDs.
    let cols:Vec<String>=sqlx::query_scalar("SELECT column_name FROM information_schema.columns WHERE table_name='mobile_operation_receipt'").fetch_all(&pool).await.unwrap();
    assert!(!cols
        .iter()
        .any(|v| matches!(v.as_str(), "body" | "title" | "payload" | "raw_request")));
}

#[sqlx::test]
#[ignore]
async fn current_authority_cross_org_receipts_and_workspace_hold(pool: PgPool) {
    let f = fixture(&pool).await;
    let (foreign_org, _) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Foreign",
        "foreign@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    let stage: Uuid = sqlx::query_scalar("SELECT id FROM stage WHERE organization_id=$1 LIMIT 1")
        .bind(foreign_org)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO person(organization_id,stage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(foreign_org)
    .bind(stage)
    .fetch_one(&f.app)
    .await
    .unwrap();
    for person in [foreign, Uuid::new_v4()] {
        assert_eq!(
            f.post(
                "/api/mobile/v1/operations",
                f.operation("add_note", json!({"person_id":person,"body":"Denied"}))
            )
            .await
            .1["error"],
            "not_found"
        );
        assert_eq!(f.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[person]})).await.1["error"],"not_found");
    }
    let create=f.operation("create_task",json!({"person_id":f.person,"title":"Shared","kind":"other","due_at":null,"assignee_user_id":null}));
    let (_, created) = f.post("/api/mobile/v1/operations", create).await;
    let task = id(&created, "resource_id");
    let complete = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"task_id":task,"expected_revision":"1"}}),
    );
    let (_, accepted) = f.post("/api/mobile/v1/operations", complete.clone()).await;
    // A prior receipt remains replayable after losing manage permission.
    sqlx::query("UPDATE task SET created_by_user_id=$1,assignee_user_id=$1 WHERE id=$2")
        .bind(f.other)
        .bind(task)
        .execute(&f.app)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", complete.clone())
            .await
            .1["replayed"],
        true
    );
    let new = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"task_id":task,"expected_revision":"1"}}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", new).await.0,
        StatusCode::FORBIDDEN
    );
    let other_cookie = crate::common::login_cookie(&f.router, "second@fixture.test", PW).await;
    assert_eq!(
        request(
            &f.router,
            &other_cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/operations/{}",
                accepted["operation_id"].as_str().unwrap()
            ),
            json!(null)
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    task::delete_task(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&AuthContext {
            actor_user_id: UserId(f.other),
            ..f.auth()
        }),
        task::DeleteTask {
            person_id: PersonId(f.person),
            task_id: TaskId(task),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", complete).await.1["error"],
        "not_found"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_operation_receipt WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            None,
            "POST",
            "/api/mobile/v1/bootstrap",
            json!({"protocol":"mobile-v1","installation_id":f.install})
        )
        .await
        .1["error"],
        "workspace_in_migration_review"
    );
}

#[sqlx::test]
#[ignore]
async fn bounded_downloads_revisions_seal_and_acknowledgment_order(pool: PgPool) {
    let f = fixture(&pool).await;
    // Representative approved fixture: 100 selected People, 1,000 notes/tasks.
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Synthetic '||n,p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,99) n WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) SELECT $1,$2,$3,'Synthetic note '||n,'web_session',gen_random_uuid() FROM generate_series(1,1000) n")
        .bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,created_by_user_id,assignee_user_id,origin,correlation_id) SELECT $1,$2,'Synthetic task '||n,'follow_up',$3,$3,'web_session',gen_random_uuid() FROM generate_series(1,1000) n")
        .bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap();
    let generation = f.gen().await;
    assert_eq!(generation["selected_count"], 100);
    assert_eq!(generation["complete"], true);
    record("reconciliation", &generation);
    record("reconciliation_bootstrap", &f.bootstrap);
    let gen = id(&generation, "generation_id");
    let (status, summary) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!(
            "/api/mobile/v1/reconciliations/{gen}/people/{}/summary",
            f.person
        ),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    record("summary_page", &summary);
    for section in ["notes", "tasks"] {
        let mut cursor = None;
        let mut count = 0;
        let mut pages = 0;
        loop {
            let url = format!(
                "/api/mobile/v1/reconciliations/{gen}/people/{}/{section}{}",
                f.person,
                cursor
                    .as_ref()
                    .map(|c| format!("?cursor={c}"))
                    .unwrap_or_default()
            );
            let (status, page) = request(
                &f.router,
                &f.cookie,
                Some(f.context),
                "GET",
                &url,
                json!(null),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{page}");
            if pages == 0 {
                record(&format!("{section}_page"), &page);
            }
            let rows = page["items"].as_array().unwrap();
            assert!(rows.len() <= 100);
            assert!(page.to_string().len() <= 524288);
            if section == "tasks" {
                assert!(rows.iter().all(|r| r["revision"] == "1"));
            }
            count += rows.len();
            pages += 1;
            cursor = page["next_cursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                assert_eq!(page["complete"], true);
                break;
            }
            assert_eq!(page["complete"], false);
            assert!(pages <= 10);
        }
        assert_eq!(count, 1000);
        assert_eq!(pages, 10);
    }
    let (status, sealed) = f
        .post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sealed}");
    record("seal", &sealed);
    // The server fixed-clock Today result is exactly the existing normal core.
    let evaluated: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(generation["evaluated_at"].clone()).unwrap();
    let today = crm_api::domain::today::query_at(
        &mut f.app.acquire().await.unwrap(),
        &crm_api::domain::person::visibility::PersonVisibilityScope::Organization(OrganizationId(
            f.org,
        )),
        UserId(f.actor),
        evaluated,
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(today).unwrap(), sealed["today"]);
    let entry = generation["manifest"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["person_id"] == f.person.to_string())
        .unwrap();
    let old_revision = entry["revision"].as_str().unwrap().parse::<i64>().unwrap();
    // A response lost after commit is retried under its immutable ID; 100
    // queued actions each settle once. Old sealed bundles cannot cover them.
    for n in 0..100 {
        let op = f.operation(
            "add_note",
            json!({"person_id":f.person,"body":format!("Offline queued {n}")}),
        );
        let (status, receipt) = f.post("/api/mobile/v1/operations", op.clone()).await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert!(
            receipt["person_revision"]
                .as_str()
                .unwrap()
                .parse::<i64>()
                .unwrap()
                > old_revision
        );
        assert_eq!(
            f.post("/api/mobile/v1/operations", op).await.1["replayed"],
            true
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1100
    );
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({})
        )
        .await
        .1["error"],
        "generation_changed"
    );
    let (status, changed) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!(
            "/api/mobile/v1/reconciliations/{gen}/people/{}/notes",
            f.person
        ),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(changed["error"], "generation_changed");
    record("generation_changed", &changed);
    // Label fanout includes note authors even when no Person is assigned.
    let before: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    sqlx::query("UPDATE app_user SET display_name='Changed Agent' WHERE id=$1")
        .bind(f.actor)
        .execute(&pool)
        .await
        .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert!(after > before);
    // Every exact response shape can be fed to native contract fixture readers.
    let (status, error) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        "/api/mobile/v1/reconciliations/not-a-uuid/manifest",
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "malformed_request");
}

#[sqlx::test]
#[ignore]
async fn trusted_admission_context_rotation_cleanup_and_cursor_scope(pool: PgPool) {
    let f = fixture(&pool).await;
    // At least two manifest pages and no dependence on caller-generated IDs.
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Manifest '||n,p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,260) n WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    let first = f.gen().await;
    let first_id = id(&first, "generation_id");
    assert_eq!(first["manifest"]["items"].as_array().unwrap().len(), 250);
    assert_eq!(first["manifest"]["complete"], false);
    let second = f.gen().await;
    let second_id = id(&second, "generation_id");
    let first_cursor = first["manifest"]["next_cursor"].as_str().unwrap();
    let (status, page) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/reconciliations/{first_id}/manifest?cursor={first_cursor}"),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 11);
    assert_eq!(page["complete"], true);
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{second_id}/manifest?cursor={first_cursor}"),
            json!(null)
        )
        .await
        .1["error"],
        "malformed_request"
    );
    assert_eq!(
        f.post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    for n in 0..9 {
        let installation = Uuid::new_v4();
        let (status, boot) = request(
            &f.router,
            &f.cookie,
            None,
            "POST",
            "/api/mobile/v1/bootstrap",
            json!({"protocol":"mobile-v1","installation_id":installation}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let context = id(&boot, "context_id");
        let (status, _) = request(
            &f.router,
            &f.cookie,
            Some(context),
            "POST",
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":installation,"pinned_person_ids":[]}),
        )
        .await;
        assert_eq!(
            status,
            if n < 2 {
                StatusCode::OK
            } else {
                StatusCode::TOO_MANY_REQUESTS
            }
        );
    }
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            None,
            "POST",
            "/api/mobile/v1/bootstrap",
            json!({"protocol":"mobile-v1","installation_id":Uuid::new_v4()})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        4
    );
    sqlx::query("UPDATE mobile_reconciliation SET expires_at=now()-interval '1 second' WHERE organization_id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{first_id}/manifest"),
            json!(null)
        )
        .await
        .1["error"],
        "generation_expired"
    );
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn query_budget_rollback_nullable_attribution_and_physical_delete(pool: PgPool) {
    let f = fixture(&pool).await;
    // The whole-request budget includes session extraction before mobile routes.
    let mut session_lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE user_session IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *session_lock)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let (status, error) = request(
        &f.router,
        &f.cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":f.install}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"], "unavailable");
    assert!(started.elapsed() >= std::time::Duration::from_secs(19));
    assert!(started.elapsed() < std::time::Duration::from_secs(25));
    session_lock.rollback().await.unwrap();

    // Legacy null attribution is legal in ordinary stored data; reads still
    // need concrete boolean permission hints for members.
    sqlx::query("INSERT INTO note(organization_id,person_id,body,origin,correlation_id) VALUES($1,$2,'Unmatched historic author','migration',gen_random_uuid())").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,origin,correlation_id) VALUES($1,$2,'Unmatched historic creator','other','migration',gen_random_uuid())").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let (status,generation)=f.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[f.person]})).await;
    assert_eq!(status, StatusCode::OK);
    let gen = id(&generation, "generation_id");
    for section in ["notes", "tasks"] {
        let (status, page) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/reconciliations/{gen}/people/{}/{section}",
                f.person
            ),
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["items"][0]["can_manage"], false);
    }
    let op = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"Slow receipt stays pending"}),
    );
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_slow_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(10); RETURN NEW; END $$; CREATE TRIGGER mobile_fixture_slow BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile_fixture_slow_receipt();").execute(&pool).await.unwrap();
    let started = std::time::Instant::now();
    assert_eq!(
        f.post("/api/mobile/v1/operations", op.clone()).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(8));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER mobile_fixture_slow ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    let (_, accepted) = f.post("/api/mobile/v1/operations", op.clone()).await;
    sqlx::query("DELETE FROM person WHERE id=$1")
        .bind(f.person)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", op).await.1["error"],
        "not_found"
    );
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/operations/{}",
                accepted["operation_id"].as_str().unwrap()
            ),
            json!(null)
        )
        .await
        .1["error"],
        "not_found"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_operation_receipt WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        1
    );
    let (_, changed) = f
        .post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({}),
        )
        .await;
    assert_eq!(changed["error"], "generation_changed");
    assert_eq!(changed["changes"]["removed"], 1);
    record("removed_pinned_person", &changed);
}

#[sqlx::test]
#[ignore]
async fn maximum_selection_plan_and_failed_cleanup_stay_bounded(pool: PgPool) {
    let f = fixture(&pool).await;
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Scale '||n,p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,24999) n WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    sqlx::query("ANALYZE person").execute(&pool).await.unwrap();
    let pins: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 ORDER BY id")
            .bind(f.org)
            .fetch_all(&f.app)
            .await
            .unwrap();
    let started = std::time::Instant::now();
    let (status, generation) = f
        .post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":pins}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{generation}");
    assert_eq!(generation["selected_count"], 25000);
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let plan:Value=sqlx::query_scalar("EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON) SELECT id,mobile_revision FROM person WHERE organization_id=$1 AND (assigned_user_id=$2 OR id=ANY($3::uuid[]) OR id=ANY($4::uuid[])) ORDER BY id LIMIT 25001")
        .bind(f.org).bind(f.actor).bind(&pins).bind(Vec::<Uuid>::new()).fetch_one(&f.app).await.unwrap();
    record(
        "selection_25000_plan",
        &json!({"planning_ms":plan[0]["Planning Time"],"execution_ms":plan[0]["Execution Time"],"root_node":plan[0]["Plan"]["Node Type"],"actual_rows":plan[0]["Plan"]["Actual Rows"],"shared_hit_blocks":plan[0]["Plan"]["Shared Hit Blocks"],"shared_read_blocks":plan[0]["Plan"]["Shared Read Blocks"]}),
    );
    assert_eq!(plan[0]["Plan"]["Actual Rows"].as_f64(), Some(25000.0));
    sqlx::query("INSERT INTO person(organization_id,stage_id,assigned_user_id) SELECT organization_id,stage_id,assigned_user_id FROM person WHERE id=$1").bind(f.person).execute(&f.app).await.unwrap();
    assert_eq!(
        f.post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]})
        )
        .await
        .1["error"],
        "over_limit"
    );
    sqlx::query("UPDATE mobile_reconciliation SET expires_at=now()-interval '1 second' WHERE organization_id=$1").bind(f.org).execute(&pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_cleanup_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic cleanup failure'; END $$; CREATE TRIGGER mobile_fixture_cleanup BEFORE DELETE ON mobile_reconciliation FOR EACH ROW EXECUTE FUNCTION mobile_fixture_cleanup_failure();").execute(&pool).await.unwrap();
    assert!(mobile::cleanup_once(&f.app).await.is_err());
    assert_eq!(
        f.post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]})
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_reconciliation_person")
            .fetch_one(&f.app)
            .await
            .unwrap(),
        25000
    );
    sqlx::query("DROP TRIGGER mobile_fixture_cleanup ON mobile_reconciliation")
        .execute(&pool)
        .await
        .unwrap();
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_reconciliation_person")
            .fetch_one(&f.app)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn concurrent_admission_and_download_slots_are_server_enforced(pool: PgPool) {
    let f = fixture(&pool).await;
    let body = json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]});
    let (a, b, c) = tokio::join!(
        f.post("/api/mobile/v1/reconciliations", body.clone()),
        f.post("/api/mobile/v1/reconciliations", body.clone()),
        f.post("/api/mobile/v1/reconciliations", body)
    );
    let results = [a, b, c];
    let successes = results.iter().filter(|r| r.0 == StatusCode::OK).count();
    assert!((1..=2).contains(&successes));
    for result in &results {
        assert!([
            StatusCode::OK,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::TOO_MANY_REQUESTS
        ]
        .contains(&result.0));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1"
        )
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        successes as i64
    );
    let gen = id(
        &results.iter().find(|r| r.0 == StatusCode::OK).unwrap().1,
        "generation_id",
    );
    let mut held = f.app.begin().await.unwrap();
    for slot in ["0", "1"] {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('crm-mobile-download:'||$1::text||':'||$2::text,0))").bind(f.context).bind(slot).execute(&mut *held).await.unwrap();
    }
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{gen}/manifest"),
            json!(null)
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    held.rollback().await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{gen}/manifest"),
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[sqlx::test]
#[ignore]
async fn derived_trigger_permissions_never_bypass_review_business_guards(pool: PgPool) {
    let f = fixture(&pool).await;
    let before: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    let direct = sqlx::query("UPDATE person SET mobile_revision=mobile_revision+1 WHERE id=$1")
        .bind(f.person)
        .execute(&f.app)
        .await
        .unwrap_err();
    assert!(crm_api::auth::workspace::is_review_error(&direct));
    let insert=sqlx::query("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) VALUES($1,$2,$3,'Must not bypass','mobile_session',gen_random_uuid())").bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap_err();
    assert!(crm_api::auth::workspace::is_review_error(&insert));
    let after: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert_eq!(before, after);
    let can_call: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('crm_app','crm_mobile_component_revision()','EXECUTE')",
    )
    .fetch_one(&f.app)
    .await
    .unwrap();
    assert!(!can_call);
}

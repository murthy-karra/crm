//! DB-backed tests for Slice 018 (docs/specs/SLICE_018.md §12): the
//! Operator's `complete_task` (executes at once, D-057 §1) and
//! `create_task` (propose → confirm, D-057 §2). Run only via
//! ./scripts/check-db. The migrator connection is used only to backdate,
//! force lifecycle states, deactivate a membership out of band, or probe
//! grants/CHECKs for negative cases — the `db_operator_call.rs`/
//! `db_notes.rs` precedent.
//!
//! Neither tool needs telephony (docs/specs/SLICE_018.md §5: `create_task`
//! confirm "needs no telephony and no operator runtime"), so this file's
//! fixture never configures a `Telephony` — every confirm here already
//! demonstrates "provider key unset → confirm still works" by construction.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::operator::OperatorRuntime;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;
use crm_operator::{
    ChatResponse, Limits, ScriptedProvider as ScriptedInference, ScriptedStep, ToolCall,
};

const PW: &str = "pw";

struct Fixture {
    migrator_pool: PgPool,
    org_id: Uuid,
    alice_id: Uuid,
    carol_id: Uuid,
    bob_id: Uuid,
    /// Has the operator runtime configured with whatever steps `fixture`
    /// or `rebuild_with_steps` was last given.
    router: Router,
    /// Same state, no operator runtime — proves confirm is model-free.
    router_no_operator: Router,
    alice: String,
    carol: String,
    /// A member of a different Organization entirely.
    bob: String,
    publisher: Publisher,
}

async fn fixture(migrator_pool: &PgPool, steps: Vec<ScriptedStep>) -> Fixture {
    let (org_id, alice_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    let carol_id = crate::common::create_user(migrator_pool, "carol@acme.test", "Carol", PW).await;
    crate::common::add_membership(migrator_pool, org_id, carol_id).await;
    let (_other_org_id, bob_id) = crate::common::create_org_with_stages_and_member(
        migrator_pool,
        "Best Realty",
        "bob@best.test",
        "Bob",
        PW,
    )
    .await;

    let publisher = Publisher::recording();
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = crate::common::test_config();
    let base = AppState::for_tests(app_pool, &config, publisher.clone());

    let inference = ScriptedInference::new(steps);
    let runtime = OperatorRuntime::with_provider(Arc::new(inference.clone()), Limits::default(), 4);
    let router = crm_api::build_app(base.clone().with_operator(runtime));
    let router_no_operator = crm_api::build_app(base);

    let alice = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let carol = crate::common::login_cookie(&router, "carol@acme.test", PW).await;
    let bob = crate::common::login_cookie(&router, "bob@best.test", PW).await;
    Fixture {
        migrator_pool: migrator_pool.clone(),
        org_id,
        alice_id,
        carol_id,
        bob_id,
        router,
        router_no_operator,
        alice,
        carol,
        bob,
        publisher,
    }
}

/// Rebuilds `router` alone with a fresh script (the ids a script needs
/// only exist once the fixture's Person/task setup has run).
async fn rebuild_with_steps(f: &Fixture, steps: Vec<ScriptedStep>) -> Router {
    let inference = ScriptedInference::new(steps);
    let runtime = OperatorRuntime::with_provider(Arc::new(inference.clone()), Limits::default(), 4);
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let config = crate::common::test_config();
    let base = AppState::for_tests(app_pool, &config, f.publisher.clone());
    crm_api::build_app(base.with_operator(runtime))
}

fn tool_call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: "c".to_string(),
        name: name.to_string(),
        arguments: args.to_string(),
    }
}

fn steps_one_call(name: &str, args: Value, reply: &str) -> Vec<ScriptedStep> {
    vec![
        ScriptedStep::Respond(ChatResponse::tool_calls(vec![tool_call(name, args)])),
        ScriptedStep::Respond(ChatResponse::text(reply)),
    ]
}

async fn run_turn(router: &Router, cookie: &str) -> Value {
    let resp = crate::common::post_json_with_cookie(
        router,
        "/api/operator/turns",
        cookie,
        json!({ "message": "go", "history": [], "context": { "route": "other" }, "utc_offset_minutes": 0 }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "turn");
    crate::common::body_json(resp).await
}

async fn confirm(router: &Router, cookie: &str, proposal_id: &str) -> axum::response::Response {
    crate::common::post_json_with_cookie(
        router,
        &format!("/api/operator/proposals/{proposal_id}/confirm"),
        cookie,
        json!({}),
    )
    .await
}

/// A bare Person via intake, assigned to `f.alice`.
async fn person(f: &Fixture, email: &str) -> Uuid {
    person_as(f, &f.alice, Some(f.alice_id), email).await
}

/// As `person`, but under an arbitrary cookie/assignee — for a Person
/// under a *different* Organization than `f.org_id` (the cookie's own
/// active Organization decides the tenant, the ordinary multi-tenant
/// `post_inquiry` precedent).
async fn person_as(f: &Fixture, cookie: &str, assignee: Option<Uuid>, email: &str) -> Uuid {
    let resp = crate::common::post_inquiry(
        &f.router,
        cookie,
        "zillow",
        json!({ "email": email, "message": "hi" }),
        assignee,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED, "intake fixture");
    crate::common::body_json(resp).await["person_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Creates one task on `person_id` as `cookie`, optionally assigned to
/// `assignee`; returns the new task's id.
async fn create_task_for(
    f: &Fixture,
    cookie: &str,
    person_id: Uuid,
    assignee: Option<Uuid>,
) -> Uuid {
    let mut body = json!({ "title": "Call about the listing", "kind": "call" });
    if let Some(a) = assignee {
        body["assignee_user_id"] = json!(a);
    }
    let resp = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/tasks"),
        cookie,
        body,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED, "task fixture");
    crate::common::body_json(resp).await["task"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn task_row(
    pool: &PgPool,
    task_id: Uuid,
) -> (Option<chrono::DateTime<chrono::Utc>>, String, Uuid) {
    sqlx::query_as("SELECT completed_at, origin, correlation_id FROM task WHERE id = $1")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn proposal_row(
    pool: &PgPool,
    id: Uuid,
) -> (String, Option<String>, Option<Uuid>, Option<Uuid>, Uuid) {
    sqlx::query_as(
        "SELECT status, failure_code, call_id, task_id, turn_id FROM operator_proposal WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn deactivate_membership(pool: &PgPool, org_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "UPDATE organization_membership SET status = 'inactive' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

// === complete_task =========================================================

/// docs/specs/SLICE_018.md §6, §12: rule 1 (assignee, creator, admin) may
/// complete; a third member may not — a positive control (the same task,
/// completed by the assignee right after) proves the tool itself works,
/// so the third member's `forbidden` is not masked by an unrelated wiring
/// bug. No write, no publication on `forbidden`.
#[sqlx::test]
#[ignore]
async fn complete_task_forbidden_for_a_third_member_with_a_positive_control(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let dave_id = crate::common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    crate::common::add_membership(&migrator_pool, f.org_id, dave_id).await;
    let dave = crate::common::login_cookie(&f.router, "dave@acme.test", PW).await;

    let person_id = person(&f, "grace@op.test").await;
    // Assigned to alice; dave is neither assignee, creator, nor admin.
    let task_id = create_task_for(&f, &f.alice, person_id, Some(f.alice_id)).await;

    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "noted",
        ),
    )
    .await;
    let before = crate::common::calls::recorded(&f.publisher).await.len();
    let out = run_turn(&router, &dave).await;
    assert!(out["receipt"].is_null(), "forbidden sets no receipt");
    assert_eq!(out["tool_calls"][0]["outcome"], "ok");
    let (completed_at, ..) = task_row(&f.migrator_pool, task_id).await;
    assert!(completed_at.is_none(), "forbidden writes nothing");
    let after = crate::common::calls::recorded(&f.publisher).await.len();
    assert_eq!(before, after, "forbidden publishes nothing");

    // Positive control: the actual assignee completes the very same task.
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "done",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert_eq!(out["receipt"]["kind"], "complete_task");
    let (completed_at, ..) = task_row(&f.migrator_pool, task_id).await;
    assert!(completed_at.is_some(), "the control completes it");
}

/// The creator and an Organization admin may also complete a task they
/// are not assigned to (rule 1's other two arms).
#[sqlx::test]
#[ignore]
async fn complete_task_permits_creator_and_admin(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    // An admin, distinct from alice (a plain member).
    let erin_id = crate::common::create_user(&migrator_pool, "erin@acme.test", "Erin", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        f.org_id,
        erin_id,
        crm_app::domain::admin::Role::Admin,
        crm_app::domain::admin::MembershipStatus::Active,
    )
    .await;
    let erin = crate::common::login_cookie(&f.router, "erin@acme.test", PW).await;

    let person_id = person(&f, "task-creator@op.test").await;
    // Created by carol, assigned to alice: carol (creator) may complete it.
    let task_id = create_task_for(&f, &f.carol, person_id, Some(f.alice_id)).await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "done",
        ),
    )
    .await;
    let out = run_turn(&router, &f.carol).await;
    assert_eq!(out["receipt"]["kind"], "complete_task");

    // A second task, assigned to carol and created by carol: erin (admin)
    // may complete it despite being neither.
    let task_id2 = create_task_for(&f, &f.carol, person_id, Some(f.carol_id)).await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id2}),
            "done",
        ),
    )
    .await;
    let out = run_turn(&router, &erin).await;
    assert_eq!(out["receipt"]["kind"], "complete_task");
}

/// Foreign Organization, another Person's path, a tombstoned task, and a
/// random id are all byte-identical `not_found` (docs/specs/SLICE_018.md
/// §3, §6).
#[sqlx::test]
#[ignore]
async fn complete_task_not_found_is_byte_identical_across_causes(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_a = person(&f, "a@op.test").await;
    let person_b = person(&f, "b@op.test").await;
    let task_on_a = create_task_for(&f, &f.alice, person_a, Some(f.alice_id)).await;

    // Tombstone a separate task.
    let tombstoned = create_task_for(&f, &f.alice, person_a, Some(f.alice_id)).await;
    let del = crate::common::delete_with_cookie(
        &f.router,
        &format!("/api/people/{person_a}/tasks/{tombstoned}"),
        &f.alice,
    )
    .await;
    assert_eq!(del.status(), StatusCode::OK);

    // A task that belongs to a Person under an entirely different
    // Organization (Best Realty) — alice's own Organization id can never
    // match this row's, regardless of which Person id she names.
    let bob_person = person_as(&f, &f.bob, Some(f.bob_id), "bob-person@op.test").await;
    let bob_task = create_task_for(&f, &f.bob, bob_person, Some(f.bob_id)).await;

    let cases = [
        // Foreign Organization: bob has no visibility into this task at all.
        (person_a, task_on_a, f.bob.clone()),
        // Another Person's path: the task exists, but not under person_b.
        (person_b, task_on_a, f.alice.clone()),
        // Tombstoned.
        (person_a, tombstoned, f.alice.clone()),
        // Random id.
        (person_a, Uuid::new_v4(), f.alice.clone()),
        // A foreign Organization's task id, reached through alice's OWN
        // Person path.
        (person_a, bob_task, f.alice.clone()),
    ];
    let mut replies = Vec::new();
    let mut turn_ids = Vec::new();
    for (pid, tid, cookie) in cases {
        let router = rebuild_with_steps(
            &f,
            steps_one_call(
                "complete_task",
                json!({"person_id": pid, "task_id": tid}),
                "noted",
            ),
        )
        .await;
        let out = run_turn(&router, &cookie).await;
        assert_eq!(out["tool_calls"][0]["outcome"], "not_found");
        assert!(out["receipt"].is_null());
        turn_ids.push(out["turn_id"].as_str().unwrap().parse::<Uuid>().unwrap());
        // Byte-identical: strip the one field every record legitimately
        // differs on (timing) and compare the rest verbatim (`person_ids`
        // is `#[serde(skip)]` — never on the wire at all, checked below
        // against the ledger instead).
        let mut record = out["tool_calls"][0].clone();
        record.as_object_mut().unwrap().remove("duration_ms");
        replies.push(record);
    }
    for record in &replies {
        assert_eq!(record["name"], "complete_task");
        assert_eq!(record["outcome"], "not_found");
    }
    for r in &replies[1..] {
        assert_eq!(r, &replies[0], "every not_found record is byte-identical");
    }

    // The ledger's own `person_ids` column (not on the wire at all,
    // `#[serde(skip)]`) is empty for every one of these calls: `not_found`
    // touches no Person.
    for turn_id in &turn_ids {
        let person_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT person_ids FROM operator_tool_call WHERE turn_id = $1")
                .bind(turn_id)
                .fetch_one(&f.migrator_pool)
                .await
                .unwrap();
        assert!(person_ids.is_empty(), "not_found touches no Person");
    }

    // No case wrote anything: alice's own task and bob's are both
    // untouched.
    let (completed_at, ..) = task_row(&f.migrator_pool, task_on_a).await;
    assert!(completed_at.is_none(), "alice's own task stays open");
    let (bob_completed_at, ..) = task_row(&f.migrator_pool, bob_task).await;
    assert!(bob_completed_at.is_none(), "bob's task stays open");
}

/// A second click that races the model: `changed: false`, no receipt-worthy
/// re-execution, no publish (docs/specs/SLICE_018.md §12 idempotency).
#[sqlx::test]
#[ignore]
async fn complete_task_already_completed_is_idempotent_and_does_not_republish(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "idempotent@op.test").await;
    let task_id = create_task_for(&f, &f.alice, person_id, Some(f.alice_id)).await;

    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "done",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert_eq!(out["receipt"]["kind"], "complete_task");
    let after_first = crate::common::calls::recorded(&f.publisher).await.len();
    assert!(after_first > 0, "the real completion publishes");

    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "already done",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert!(
        out["receipt"].is_null(),
        "already_completed sets no receipt"
    );
    assert_eq!(out["tool_calls"][0]["outcome"], "ok");
    let after_second = crate::common::calls::recorded(&f.publisher).await.len();
    assert_eq!(
        after_first, after_second,
        "no republish on already_completed"
    );
}

/// `tokio::join!` an Operator turn against the panel's own complete route,
/// racing on the same task: exactly one `changed: true` (docs/specs/
/// SLICE_018.md §12).
#[sqlx::test]
#[ignore]
async fn operator_versus_panel_completion_gives_one_changed_true(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "race@op.test").await;
    let task_id = create_task_for(&f, &f.alice, person_id, Some(f.alice_id)).await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "done",
        ),
    )
    .await;

    let complete_uri = format!("/api/people/{person_id}/tasks/{task_id}/complete");
    let turn_fut = run_turn(&router, &f.alice);
    let panel_fut =
        crate::common::post_json_with_cookie(&router, &complete_uri, &f.alice, json!({}));
    let (turn_out, panel_resp) = tokio::join!(turn_fut, panel_fut);

    assert_eq!(panel_resp.status(), StatusCode::OK);
    let panel_body = crate::common::body_json(panel_resp).await;
    let panel_changed = panel_body["changed"].as_bool().unwrap();
    // The Operator's own call succeeds either way it races (Completed or
    // AlreadyCompleted are both an "ok" tool outcome) — only the receipt
    // distinguishes which one actually happened.
    assert_eq!(turn_out["tool_calls"][0]["outcome"], "ok");
    let operator_changed = !turn_out["receipt"].is_null();

    assert_ne!(
        panel_changed, operator_changed,
        "exactly one side wins the race: panel={panel_changed} operator={operator_changed}"
    );
    let (completed_at, ..) = task_row(&f.migrator_pool, task_id).await;
    assert!(completed_at.is_some());
}

/// The task row's `origin` is untouched by a completion; the ledger's
/// `operator_tool_call` row never carries a title (docs/specs/SLICE_018.md
/// §11, D-029).
#[sqlx::test]
#[ignore]
async fn complete_task_leaves_origin_unchanged_and_the_ledger_carries_no_title(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "origin@op.test").await;
    let task_id = create_task_for(&f, &f.alice, person_id, Some(f.alice_id)).await;
    let (_, origin_before, _) = task_row(&f.migrator_pool, task_id).await;
    assert_eq!(origin_before, "web_session");

    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "complete_task",
            json!({"person_id": person_id, "task_id": task_id}),
            "SECRET-TITLE-SHOULD-NEVER-BE-LOGGED",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let turn_id: Uuid = out["turn_id"].as_str().unwrap().parse().unwrap();

    let (_, origin_after, _) = task_row(&f.migrator_pool, task_id).await;
    assert_eq!(
        origin_after, "web_session",
        "completion never rewrites origin"
    );

    let row: (String,) =
        sqlx::query_as("SELECT tool_name FROM operator_tool_call WHERE turn_id = $1")
            .bind(turn_id)
            .fetch_one(&f.migrator_pool)
            .await
            .unwrap();
    assert_eq!(row.0, "complete_task");
    // The ledger row's columns are id/tool_name/outcome/duration_ms/
    // person_ids only (docs/specs/SLICE_005.md §2) — no title-shaped
    // column exists to carry one; the model's canned reply text (which
    // could have echoed a title) never reaches this table either.
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns WHERE table_name = 'operator_tool_call'",
    )
    .fetch_all(&f.migrator_pool)
    .await
    .unwrap();
    for forbidden in ["title", "reply", "message"] {
        assert!(
            !cols.iter().any(|c| c == forbidden),
            "unexpected column: {forbidden}"
        );
    }
}

// === create_task ============================================================

/// The proposal chain's parent and sidecar row shapes (docs/specs/
/// SLICE_018.md §4, §12): `operator_proposal` carries no title;
/// `operator_task_proposal` carries the title, and both rows agree on
/// `person_id`/`organization_id`.
#[sqlx::test]
#[ignore]
async fn create_task_proposal_inserts_parent_and_sidecar_rows(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "propose@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Call Grace"}),
            "Ready to confirm.",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert_eq!(out["proposal"]["kind"], "create_task");
    assert_eq!(out["proposal"]["title"], "Call Grace");
    assert_eq!(out["proposal"]["task_kind"], "follow_up");
    assert_eq!(out["proposal"]["assignee"]["id"], f.alice_id.to_string());
    let proposal_id: Uuid = out["proposal"]["id"].as_str().unwrap().parse().unwrap();

    let (status, failure, call_id, task_id, _turn_id) =
        proposal_row(&f.migrator_pool, proposal_id).await;
    assert_eq!(status, "proposed");
    assert_eq!(failure, None);
    assert_eq!(call_id, None);
    assert_eq!(task_id, None);

    let (sidecar_org, sidecar_person, sidecar_title, sidecar_kind, sidecar_assignee): (
        Uuid,
        Uuid,
        String,
        String,
        Uuid,
    ) = sqlx::query_as(
        "SELECT organization_id, person_id, title, kind, assignee_user_id
         FROM operator_task_proposal WHERE proposal_id = $1",
    )
    .bind(proposal_id)
    .fetch_one(&f.migrator_pool)
    .await
    .unwrap();
    assert_eq!(sidecar_org, f.org_id);
    assert_eq!(sidecar_person, person_id);
    assert_eq!(sidecar_title, "Call Grace");
    assert_eq!(sidecar_kind, "follow_up");
    assert_eq!(sidecar_assignee, f.alice_id);

    let (parent_tool, parent_cm): (String, Option<Uuid>) =
        sqlx::query_as("SELECT tool, contact_method_id FROM operator_proposal WHERE id = $1")
            .bind(proposal_id)
            .fetch_one(&f.migrator_pool)
            .await
            .unwrap();
    assert_eq!(parent_tool, "create_task");
    assert_eq!(
        parent_cm, None,
        "create_task never carries a contact_method_id"
    );
}

/// Confirm needs neither telephony nor the operator runtime; the created
/// task carries `origin='operator'` and `correlation_id = turn_id`
/// (docs/specs/SLICE_018.md §4, §5).
#[sqlx::test]
#[ignore]
async fn create_task_confirm_without_operator_runtime_sets_origin_and_correlation(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "confirm@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Call Grace"}),
            "Ready.",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();
    let (_, _, _, _, turn_id) = proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;

    // Confirm on the router WITHOUT an operator runtime: model-free.
    let resp = confirm(&f.router_no_operator, &f.alice, &proposal_id).await;
    assert_eq!(resp.status(), StatusCode::CREATED, "confirm");
    let body = crate::common::body_json(resp).await;
    let task_id: Uuid = body["task"]["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["task"]["title"], "Call Grace");

    let (completed_at, origin, correlation_id) = task_row(&f.migrator_pool, task_id).await;
    assert!(completed_at.is_none());
    assert_eq!(origin, "operator");
    assert_eq!(correlation_id, turn_id);

    let (status, _, _, row_task_id, _) =
        proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;
    assert_eq!(status, "confirmed");
    assert_eq!(row_task_id, Some(task_id));
}

/// A full proposal with `kind`, `due_date` (composed at the turn's
/// `utc_offset_minutes: 0`, end of day since no `due_time`), and a named
/// assignee — the DB row and the wire response agree with each other and
/// with the composed instant (docs/specs/SLICE_018.md §3, §4, §5).
#[sqlx::test]
#[ignore]
async fn create_task_with_due_date_and_named_assignee_composes_and_confirms(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "due-date@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({
                "person_id": person_id,
                "title": "Call Carol's client",
                "kind": "call",
                "due_date": "2026-09-12",
                "assignee": "Carol",
            }),
            "Ready.",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();
    let (_, _, _, _, turn_id) = proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;

    let resp = confirm(&f.router_no_operator, &f.alice, &proposal_id).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = crate::common::body_json(resp).await;
    let task_id: Uuid = body["task"]["id"].as_str().unwrap().parse().unwrap();

    let expected_due: chrono::DateTime<chrono::Utc> = "2026-09-12T23:59:59Z".parse().unwrap();
    let wire_due: chrono::DateTime<chrono::Utc> =
        body["task"]["due_at"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        wire_due, expected_due,
        "the wire due_at matches the composed instant"
    );

    let (kind, assignee_user_id, due_at): (String, Uuid, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT kind, assignee_user_id, due_at FROM task WHERE id = $1")
            .bind(task_id)
            .fetch_one(&f.migrator_pool)
            .await
            .unwrap();
    assert_eq!(kind, "call");
    assert_eq!(assignee_user_id, f.carol_id);
    assert_eq!(
        due_at, expected_due,
        "the stored due_at matches the composed instant"
    );

    let (completed_at, origin, correlation_id) = task_row(&f.migrator_pool, task_id).await;
    assert!(completed_at.is_none());
    assert_eq!(origin, "operator");
    assert_eq!(correlation_id, turn_id);
}

/// A Person deleted between propose and confirm cascades the sidecar
/// away; the scoped claim still succeeds, the sidecar read finds no row
/// → finalize `failed` with `not_found`, answer 404 (docs/specs/
/// SLICE_018.md §5's stated branch case).
#[sqlx::test]
#[ignore]
async fn create_task_confirm_after_person_deleted_is_404(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "deleted@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Call Grace"}),
            "Ready.",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();

    sqlx::query("DELETE FROM person WHERE id = $1")
        .bind(person_id)
        .execute(&f.migrator_pool)
        .await
        .unwrap();
    let sidecar_gone: i64 =
        sqlx::query_scalar("SELECT count(*) FROM operator_task_proposal WHERE proposal_id = $1")
            .bind(proposal_id.parse::<Uuid>().unwrap())
            .fetch_one(&f.migrator_pool)
            .await
            .unwrap();
    assert_eq!(sidecar_gone, 0, "the cascade removed the sidecar");

    let resp = confirm(&f.router_no_operator, &f.alice, &proposal_id).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let (status, failure, ..) = proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;
    assert_eq!(status, "failed");
    assert_eq!(failure, Some("not_found".to_string()));
}

/// The CHECK matrix (docs/specs/SLICE_018.md §4): `contact_method_id` is
/// non-null iff `tool = 'start_call'`; the confirmed CHECK requires
/// `task_id` for `create_task` and `call_id` for `start_call`.
#[sqlx::test]
#[ignore]
async fn create_task_check_matrix(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "checks@op.test").await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    /// The exact CHECK constraint name a rejected insert violated (review
    /// round 1: `is_err()` alone cannot tell one constraint from another).
    fn violated_constraint(err: sqlx::Error) -> String {
        match err {
            sqlx::Error::Database(e) => e
                .constraint()
                .expect("a named CHECK constraint")
                .to_string(),
            other => panic!("expected a database constraint violation, got {other:?}"),
        }
    }

    // A create_task row with a contact_method_id violates the check.
    let rejected = sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'create_task', $5, $6, 'proposed', now() + interval '2 minutes')",
    )
    .bind(Uuid::new_v4())
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(Uuid::new_v4())
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert_eq!(
        violated_constraint(rejected),
        "operator_proposal_contact_method_id_check"
    );

    // A create_task row confirmed without a task_id violates the check.
    let rejected_confirmed = sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, status, expires_at, confirmed_at)
         VALUES ($1, $2, $3, $4, 'create_task', $5, NULL, 'confirmed', now() + interval '2 minutes', now())",
    )
    .bind(Uuid::new_v4())
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert_eq!(
        violated_constraint(rejected_confirmed),
        "operator_proposal_check1"
    );

    // An invalid tool value is rejected.
    let rejected_tool = sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'send_text', $5, NULL, 'proposed', now() + interval '2 minutes')",
    )
    .bind(Uuid::new_v4())
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert_eq!(
        violated_constraint(rejected_tool),
        "operator_proposal_tool_check"
    );

    // A start_call row (a random contact_method_id — the CHECK it would
    // otherwise satisfy) with task_id also set: the per-tool hygiene CHECK
    // catches it. `status = 'claimed'` (not `'proposed'`) isolates this
    // from the proposed-status CHECK below, which would ALSO reject a
    // `'proposed'` row carrying a non-null `task_id` regardless of tool —
    // this row must violate exactly one CHECK, this one.
    let rejected_start_call_task_id = sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, task_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'start_call', $5, $6, $7, 'claimed', now() + interval '2 minutes')",
    )
    .bind(Uuid::new_v4())
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert_eq!(
        violated_constraint(rejected_start_call_task_id),
        "operator_proposal_task_id_tool_check"
    );

    // A create_task row with call_id set: the other per-tool hygiene
    // CHECK — `status = 'claimed'`, the same isolation reasoning.
    let rejected_create_task_call_id = sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, call_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'create_task', $5, NULL, $6, 'claimed', now() + interval '2 minutes')",
    )
    .bind(Uuid::new_v4())
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(Uuid::new_v4())
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert_eq!(
        violated_constraint(rejected_create_task_call_id),
        "operator_proposal_call_id_tool_check"
    );

    // A create_task row still `status = 'proposed'` with task_id already
    // set violates the proposed-status CHECK (task_id is one of the four
    // columns that CHECK requires NULL while proposed).
    let rejected_proposed_with_task_id = sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, task_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'create_task', $5, NULL, $6, 'proposed', now() + interval '2 minutes')",
    )
    .bind(Uuid::new_v4())
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .bind(Uuid::new_v4())
    .execute(&app_pool)
    .await
    .unwrap_err();
    assert_eq!(
        violated_constraint(rejected_proposed_with_task_id),
        "operator_proposal_check"
    );

    // Positive control: a well-formed create_task parent + sidecar pair
    // inserts cleanly — proving the six rejections above are genuinely
    // about the one violated field each, not some other structural error.
    let proposal_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'create_task', $5, NULL, 'proposed', now() + interval '2 minutes')",
    )
    .bind(proposal_id)
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&app_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO operator_task_proposal
           (proposal_id, organization_id, person_id, title, kind, due_at, assignee_user_id)
         VALUES ($1, $2, $3, 'Well-formed', 'follow_up', NULL, $4)",
    )
    .bind(proposal_id)
    .bind(f.org_id)
    .bind(person_id)
    .bind(f.alice_id)
    .execute(&app_pool)
    .await
    .unwrap();
}

/// `crm_app` has no UPDATE and no DELETE on the sidecar (docs/specs/
/// SLICE_018.md §4).
#[sqlx::test]
#[ignore]
async fn operator_task_proposal_grants_admit_no_update_or_delete(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let update = sqlx::query("UPDATE operator_task_proposal SET title = title WHERE false")
        .execute(&app_pool)
        .await
        .unwrap_err();
    match update {
        sqlx::Error::Database(e) => assert_eq!(e.code().as_deref(), Some("42501")),
        other => panic!("expected a permission-denied database error, got {other:?}"),
    }
    let delete = sqlx::query("DELETE FROM operator_task_proposal WHERE false")
        .execute(&app_pool)
        .await
        .unwrap_err();
    match delete {
        sqlx::Error::Database(e) => assert_eq!(e.code().as_deref(), Some("42501")),
        other => panic!("expected a permission-denied database error, got {other:?}"),
    }
}

/// Expired, consumed, double, and stuck-claimed (docs/specs/SLICE_006b.md
/// §13 precedent, extended to `create_task`): consumed beats expired; a
/// crashed claim reads as consumed; `proposal_consumed` carries `task_id`
/// after a confirmed create and null while merely `claimed` (docs/specs/
/// SLICE_018.md §5's CONTRACT item).
#[sqlx::test]
#[ignore]
async fn create_task_expired_consumed_and_stuck_claimed(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "lifecycle@op.test").await;

    // Expired.
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Expired task"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let expired_id = out["proposal"]["id"].as_str().unwrap().to_string();
    sqlx::query(
        "UPDATE operator_proposal SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(expired_id.parse::<Uuid>().unwrap())
    .execute(&f.migrator_pool)
    .await
    .unwrap();
    let resp = confirm(&f.router_no_operator, &f.alice, &expired_id).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "error": "proposal_expired" })
    );

    // Stuck-claimed (a crash between claim and finalize): reads as
    // consumed with a null task_id.
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Stuck task"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let stuck_id = out["proposal"]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE operator_proposal SET status = 'claimed' WHERE id = $1")
        .bind(stuck_id.parse::<Uuid>().unwrap())
        .execute(&f.migrator_pool)
        .await
        .unwrap();
    let resp = confirm(&f.router_no_operator, &f.alice, &stuck_id).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "error": "proposal_consumed", "call_id": null, "task_id": null })
    );

    // Consumed (already confirmed): reads as consumed, task_id populated.
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Confirmed task"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let confirmed_id = out["proposal"]["id"].as_str().unwrap().to_string();
    let first = confirm(&f.router_no_operator, &f.alice, &confirmed_id).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let task_id = crate::common::body_json(first).await["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second = confirm(&f.router_no_operator, &f.alice, &confirmed_id).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(
        crate::common::body_json(second).await,
        json!({ "error": "proposal_consumed", "call_id": null, "task_id": task_id })
    );
}

/// A `tokio::join!` double-confirm serializes on the claim: exactly one
/// 201, the other 409 `proposal_consumed`.
#[sqlx::test]
#[ignore]
async fn create_task_double_confirm_race_yields_one_201(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "double@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Race task"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();

    let a = confirm(&f.router_no_operator, &f.alice, &proposal_id);
    let b = confirm(&f.router_no_operator, &f.alice, &proposal_id);
    let (ra, rb) = tokio::join!(a, b);
    let statuses = [ra.status(), rb.status()];
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1,
        "{statuses:?}"
    );

    let task_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM task WHERE organization_id = $1 AND person_id = $2",
    )
    .bind(f.org_id)
    .bind(person_id)
    .fetch_one(&f.migrator_pool)
    .await
    .unwrap();
    assert_eq!(task_count, 1, "exactly one task row was created");
}

/// The assignee deactivated between propose and confirm: 422
/// `invalid_assignee`, proposal `failed`.
#[sqlx::test]
#[ignore]
async fn create_task_race_assignee_deactivated_is_422(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "assignee-race@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "For carol", "assignee": "Carol"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert_eq!(out["proposal"]["assignee"]["id"], f.carol_id.to_string());
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();

    deactivate_membership(&f.migrator_pool, f.org_id, f.carol_id).await;
    let resp = confirm(&f.router_no_operator, &f.alice, &proposal_id).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        crate::common::body_json(resp).await,
        json!({ "error": "invalid_assignee" })
    );
    let (status, failure, ..) = proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;
    assert_eq!(status, "failed");
    assert_eq!(failure, Some("invalid_assignee".to_string()));
}

/// The confirming actor deactivated between propose and confirm.
///
/// Through HTTP this is caught one layer earlier than the command's own
/// pass-through: `AuthContext`'s extractor re-verifies the session's
/// membership on every request (`auth/extractors.rs`'s `resolve_session`),
/// so a deactivated actor's very next request — including this confirm —
/// is already 401 `Unauthenticated` before the route body ever runs; the
/// proposal is untouched (still `proposed`), not finalized `failed`.
/// `create_task`'s own 403 `Forbidden` pass-through (docs/specs/
/// SLICE_018.md §10's table) is the command's re-check under the actor's
/// own membership lock for the case that *does* reach it (a deactivation
/// racing inside the same request's transaction window) — exercised here
/// directly against the command, the `db_notes.rs` out-of-band-UPDATE
/// pattern, since a sub-millisecond intra-request race isn't otherwise
/// reproducible in an integration test.
#[sqlx::test]
#[ignore]
async fn create_task_actor_deactivated_401_at_the_door_forbidden_at_the_command(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "actor-race@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Solo task"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();

    deactivate_membership(&f.migrator_pool, f.org_id, f.alice_id).await;
    let resp = confirm(&f.router_no_operator, &f.alice, &proposal_id).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "the auth gate fires first"
    );
    let (status, ..) = proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;
    assert_eq!(
        status, "proposed",
        "an unauthenticated confirm claims nothing"
    );

    // The command's own Forbidden pass-through, exercised directly.
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let ctx = crm_api::domain::envelope::CommandContext {
        organization_id: crm_api::ids::OrganizationId::new(f.org_id),
        actor_user_id: crm_api::ids::UserId::new(f.alice_id),
        origin: crm_api::domain::envelope::Origin::Operator,
        correlation_id: crm_api::ids::CorrelationId::new(Uuid::new_v4()),
    };
    let result = crm_api::domain::task::create_task(
        &app_pool,
        &f.publisher,
        &ctx,
        crm_api::domain::task::CreateTask {
            person_id: crm_api::ids::PersonId::new(person_id),
            title: "Direct-command race".to_string(),
            kind: crm_api::domain::task::TaskKind::default(),
            due_at: None,
            assignee_user_id: None,
        },
    )
    .await;
    assert!(
        matches!(result, Err(crm_api::domain::task::TaskError::Forbidden)),
        "{result:?}"
    );
}

/// (TRUST) Another member of the same Organization cannot confirm alice's
/// proposal (the claim's `actor_user_id` bind); the same user, under a
/// session whose active Organization differs, also cannot (the claim's
/// `organization_id` bind) — both 404 (docs/specs/SLICE_018.md §12).
#[sqlx::test]
#[ignore]
async fn create_task_confirm_by_another_member_and_by_the_same_user_elsewhere_is_404(
    migrator_pool: PgPool,
) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "cross-tenant@op.test").await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Cross-tenant"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    let proposal_id = out["proposal"]["id"].as_str().unwrap().to_string();

    // Another member of the SAME Organization: carol is not the proposal's
    // actor_user_id.
    let by_carol = confirm(&f.router_no_operator, &f.carol, &proposal_id).await;
    assert_eq!(by_carol.status(), StatusCode::NOT_FOUND);
    let (status_after_carol, ..) =
        proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;
    assert_eq!(
        status_after_carol, "proposed",
        "an unmatched confirm claims nothing"
    );

    // The SAME user (alice), but her session's active Organization is
    // switched to a second Organization she also belongs to — the same
    // cookie continues to authenticate her, now under a different
    // organization_id, which the claim's WHERE clause also binds.
    let (org_c, _org_c_admin) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Org C",
        "orgc-admin@op.test",
        "OrgC Admin",
        PW,
    )
    .await;
    crate::common::add_membership(&migrator_pool, org_c, f.alice_id).await;
    sqlx::query("UPDATE user_session SET active_organization_id = $1 WHERE user_id = $2")
        .bind(org_c)
        .bind(f.alice_id)
        .execute(&f.migrator_pool)
        .await
        .unwrap();
    let by_alice_other_org = confirm(&f.router_no_operator, &f.alice, &proposal_id).await;
    assert_eq!(by_alice_other_org.status(), StatusCode::NOT_FOUND);
    let (status_after, ..) = proposal_row(&f.migrator_pool, proposal_id.parse().unwrap()).await;
    assert_eq!(
        status_after, "proposed",
        "still untouched — still 404 the ordinary way"
    );
}

/// Assignee resolution: `"me"`, an exact display name, an ambiguous name,
/// and an inactive member all resolve correctly (docs/specs/SLICE_018.md
/// §3) — `needs_clarification` writes no row.
#[sqlx::test]
#[ignore]
async fn create_task_assignee_resolution_matrix(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "assignee-matrix@op.test").await;

    // "me" (default, omitted).
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Default me"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert_eq!(out["proposal"]["assignee"]["id"], f.alice_id.to_string());

    // Exact display name.
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "For Carol", "assignee": "Carol"}),
            "ok",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert_eq!(out["proposal"]["assignee"]["id"], f.carol_id.to_string());

    // Ambiguous: two members named "Pat".
    let pat1 = crate::common::create_user(&migrator_pool, "pat1@acme.test", "Pat", PW).await;
    crate::common::add_membership(&migrator_pool, f.org_id, pat1).await;
    let pat2 = crate::common::create_user(&migrator_pool, "pat2@acme.test", "Pat", PW).await;
    crate::common::add_membership(&migrator_pool, f.org_id, pat2).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Ambiguous", "assignee": "Pat"}),
            "which Pat?",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert!(out["proposal"].is_null());
    assert_eq!(out["tool_calls"][0]["outcome"], "ok");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    assert_eq!(before, after, "needs_clarification writes no row");

    // A name that resolves to a member of a DIFFERENT Organization: unknown
    // (never leaked across the tenant boundary) — "Bob" is a real member's
    // display name, just not of this Organization.
    let before_bob: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "For Bob", "assignee": "Bob"}),
            "who is Bob?",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert!(out["proposal"].is_null());
    assert_eq!(
        out["tool_calls"][0]["outcome"], "ok",
        "needs_clarification is not a strike"
    );
    let after_bob: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    assert_eq!(
        before_bob, after_bob,
        "a foreign-Organization member's name writes no row"
    );

    // Inactive member named.
    let before_inactive: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    deactivate_membership(&f.migrator_pool, f.org_id, f.carol_id).await;
    let router = rebuild_with_steps(
        &f,
        steps_one_call(
            "create_task",
            json!({"person_id": person_id, "title": "Inactive", "assignee": "Carol"}),
            "not active",
        ),
    )
    .await;
    let out = run_turn(&router, &f.alice).await;
    assert!(out["proposal"].is_null());
    assert_eq!(
        out["tool_calls"][0]["outcome"], "ok",
        "needs_clarification is not a strike"
    );
    let after_inactive: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    assert_eq!(
        before_inactive, after_inactive,
        "a deactivated member's name writes no row"
    );
}

// === Undo (through the existing reopen route) ==============================

/// Undo goes through the ordinary reopen route unchanged (docs/specs/
/// SLICE_018.md §4: "no marker"); `changed:false` (already reopened) is
/// still a 200.
#[sqlx::test]
#[ignore]
async fn undo_through_reopen_changed_true_then_false_then_404_and_403(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "undo@op.test").await;
    let task_id = create_task_for(&f, &f.alice, person_id, Some(f.alice_id)).await;
    let complete = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/tasks/{task_id}/complete"),
        &f.alice,
        json!({}),
    )
    .await;
    assert_eq!(complete.status(), StatusCode::OK);

    let reopen = |cookie: String| {
        let router = f.router.clone();
        async move {
            crate::common::post_json_with_cookie(
                &router,
                &format!("/api/people/{person_id}/tasks/{task_id}/reopen"),
                &cookie,
                json!({}),
            )
            .await
        }
    };

    let first = reopen(f.alice.clone()).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(crate::common::body_json(first).await["changed"], true);

    let second = reopen(f.alice.clone()).await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(crate::common::body_json(second).await["changed"], false);

    let missing = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/tasks/{}/reopen", Uuid::new_v4()),
        &f.alice,
        json!({}),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let not_found_by_bob = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/tasks/{task_id}/reopen"),
        &f.bob,
        json!({}),
    )
    .await;
    assert_eq!(
        not_found_by_bob.status(),
        StatusCode::NOT_FOUND,
        "bob cannot see this Person at all"
    );

    // A same-Organization member with no rule-1 standing (not the
    // assignee, not the creator, not an admin) sees the Person and task,
    // but is forbidden from the write.
    let dave_id = crate::common::create_user(&migrator_pool, "dave@acme.test", "Dave", PW).await;
    crate::common::add_membership(&migrator_pool, f.org_id, dave_id).await;
    let dave = crate::common::login_cookie(&f.router, "dave@acme.test", PW).await;
    let forbidden_by_dave = crate::common::post_json_with_cookie(
        &f.router,
        &format!("/api/people/{person_id}/tasks/{task_id}/reopen"),
        &dave,
        json!({}),
    )
    .await;
    assert_eq!(forbidden_by_dave.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        crate::common::body_json(forbidden_by_dave).await,
        json!({ "error": "forbidden" })
    );
}

// === start_call regression under the rewritten CHECKs =======================

/// The one assertion (docs/specs/SLICE_018.md §12): a `start_call` row
/// with a `NULL` `contact_method_id` is unreachable through the ordinary
/// insert path (the `operator_proposal_contact_method_id_check` CHECK
/// rejects it) — constructed here only by dropping that CHECK on the
/// migrator connection for this one throwaway database, inserting the
/// malformed row directly, and confirming: the route maps the impossible
/// `None` to 503 rather than panicking.
#[sqlx::test]
#[ignore]
async fn start_call_confirm_with_a_null_contact_method_id_is_503(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let person_id = person(&f, "corrupt@op.test").await;

    sqlx::query(
        "ALTER TABLE operator_proposal DROP CONSTRAINT operator_proposal_contact_method_id_check",
    )
    .execute(&f.migrator_pool)
    .await
    .unwrap();
    let proposal_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO operator_proposal
           (id, organization_id, actor_user_id, turn_id, tool, person_id,
            contact_method_id, status, expires_at)
         VALUES ($1, $2, $3, $4, 'start_call', $5, NULL, 'proposed', now() + interval '2 minutes')",
    )
    .bind(proposal_id)
    .bind(f.org_id)
    .bind(f.alice_id)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(&f.migrator_pool)
    .await
    .unwrap();

    let resp = confirm(&f.router_no_operator, &f.alice, &proposal_id.to_string()).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (status, failure, ..) = proposal_row(&f.migrator_pool, proposal_id).await;
    assert_eq!(status, "failed");
    assert_eq!(failure, Some("corrupt".to_string()));
}

// === turn request validation ================================================

/// docs/specs/SLICE_018.md §3, §5, §12: `utc_offset_minutes` outside
/// ±840, or non-integer, is 400 `malformed_request` — before any turn
/// reaches the ledger (`validate()` runs before the spawned turn task that
/// would insert the `operator_turn` row).
#[sqlx::test]
#[ignore]
async fn turn_rejects_invalid_utc_offset_minutes(migrator_pool: PgPool) {
    let f = fixture(&migrator_pool, Vec::new()).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_turn")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();

    for offset in [json!(841), json!(-841), json!(1.5), json!("60")] {
        let resp = crate::common::post_json_with_cookie(
            &f.router,
            "/api/operator/turns",
            &f.alice,
            json!({
                "message": "go",
                "history": [],
                "context": { "route": "other" },
                "utc_offset_minutes": offset,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "offset {offset}");
        assert_eq!(
            crate::common::body_json(resp).await["error"],
            "malformed_request",
            "offset {offset}"
        );
    }

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_turn")
        .fetch_one(&f.migrator_pool)
        .await
        .unwrap();
    assert_eq!(before, after, "a rejected turn never reaches the ledger");
}

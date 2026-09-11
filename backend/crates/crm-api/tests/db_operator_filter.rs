//! DB-backed tests for `filter_people`/`run_saved_list` (docs/specs/
//! SLICE_013.md §8.4-8.13). Every turn is driven by a `ScriptedProvider` —
//! no network, no model. People, tags, and saved lists are created through
//! the real command path (D-021); direct SQL is limited to fixture people
//! and stage lookups, mirroring `db_operator.rs`/`db_people_filter.rs`. Run
//! only via ./scripts/check-db.

use std::sync::Arc;

use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::sort::{PersonSort, SortDirection, SortKey};
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::tag::{self, AddPersonTag, CreateTag, DeleteTag};
use crm_api::ids::{CorrelationId, OrganizationId, PersonId, SavedListId, TagId, UserId};
use crm_api::operator::OperatorRuntime;
use crm_api::realtime::Publisher;
use crm_api::state::AppState;
use crm_operator::{ChatMessage, ChatResponse, Limits, ScriptedProvider, ScriptedStep, ToolCall};

const PW: &str = "correct horse battery staple";

// --- Fixtures ---------------------------------------------------------

struct Fixture {
    migrator_pool: PgPool,
    org: Uuid,
    alice_id: Uuid,
    bob_id: Uuid,
    stage_lead: Uuid,
}

async fn fixture(migrator_pool: PgPool) -> Fixture {
    let (org, alice_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice@acme.test",
        "Alice",
        PW,
    )
    .await;
    promote_to_admin(&migrator_pool, org, alice_id).await;
    let bob_id = crate::common::create_user(&migrator_pool, "bob@acme.test", "Bob", PW).await;
    crate::common::add_membership_with(
        &migrator_pool,
        org,
        bob_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let stage_lead = first_stage_id(&migrator_pool, org, "Lead").await;
    Fixture {
        migrator_pool,
        org,
        alice_id,
        bob_id,
        stage_lead,
    }
}

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

async fn first_stage_id(pool: &PgPool, organization_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 AND name = $2")
        .bind(organization_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_named_person(
    pool: &PgPool,
    org: Uuid,
    stage_id: Uuid,
    first_name: &str,
    last_name: &str,
    assigned_user_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO person (organization_id, stage_id, first_name, last_name, assigned_user_id)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(org)
    .bind(stage_id)
    .bind(first_name)
    .bind(last_name)
    .bind(assigned_user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_bare_people_batch(pool: &PgPool, org: Uuid, stage_id: Uuid, count: i64) {
    sqlx::query(
        "INSERT INTO person (organization_id, stage_id, created_at)
         SELECT $1, $2, now() - make_interval(secs => s.i)
         FROM generate_series(0, $3 - 1) AS s(i)",
    )
    .bind(org)
    .bind(stage_id)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_contact_attempt(
    pool: &PgPool,
    org: Uuid,
    person_id: Uuid,
    occurred_at: chrono::DateTime<chrono::Utc>,
) {
    sqlx::query(
        "INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
         VALUES ($1, 'system', NULL, 'migration', $2, $3, $4, 'call', 'reached')",
    )
    .bind(org)
    .bind(occurred_at)
    .bind(Uuid::new_v4())
    .bind(person_id)
    .execute(pool)
    .await
    .unwrap();
}

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

async fn create_tag(pool: &PgPool, org: Uuid, actor_user_id: Uuid, name: &str) -> TagId {
    tag::create_tag(
        pool,
        &command_context(org, actor_user_id),
        CreateTag {
            name: name.to_string(),
        },
    )
    .await
    .unwrap()
    .tag
    .id
}

async fn apply_tag(pool: &PgPool, org: Uuid, actor_user_id: Uuid, person_id: Uuid, tag_id: TagId) {
    tag::add_person_tag(
        pool,
        &Publisher::recording(),
        &command_context(org, actor_user_id),
        AddPersonTag {
            person_id: PersonId::new(person_id),
            tag_id,
        },
    )
    .await
    .unwrap();
}

async fn delete_tag(pool: &PgPool, org: Uuid, actor_user_id: Uuid, tag_id: TagId) {
    tag::delete_tag(
        pool,
        &command_context(org, actor_user_id),
        DeleteTag { tag_id },
    )
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn create_list(
    pool: &PgPool,
    org: Uuid,
    actor_user_id: Uuid,
    scope: SavedListScope,
    name: &str,
    clauses: Vec<crm_api::domain::person::filter::Clause>,
    sort: Option<PersonSort>,
) -> SavedListId {
    saved_list::create_saved_list(
        pool,
        &command_context(org, actor_user_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope,
            name: name.to_string(),
            filter: crm_api::domain::person::filter::FilterDefinition {
                version: 1,
                clauses,
            },
            sort,
        },
    )
    .await
    .unwrap()
    .list
    .id
}

fn provider(steps: Vec<ScriptedStep>) -> ScriptedProvider {
    ScriptedProvider::new(steps)
}

fn runtime(provider: &ScriptedProvider, max_concurrent: usize) -> OperatorRuntime {
    OperatorRuntime::with_provider(
        Arc::new(provider.clone()),
        Limits::default(),
        max_concurrent,
    )
}

async fn router_scripted(
    migrator_pool: &PgPool,
    steps: Vec<ScriptedStep>,
) -> (Router, ScriptedProvider) {
    let provider = provider(steps);
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = crate::common::test_config();
    let state = AppState::for_tests(app_pool, &config, Publisher::recording())
        .with_operator(runtime(&provider, 4));
    (crm_api::build_app(state), provider)
}

async fn router_scripted_with_publisher(
    migrator_pool: &PgPool,
    steps: Vec<ScriptedStep>,
) -> (Router, ScriptedProvider, Publisher) {
    let provider = provider(steps);
    let app_pool = crate::common::connect_as_app(migrator_pool).await;
    let config = crate::common::test_config();
    let publisher = Publisher::recording();
    let state = AppState::for_tests(app_pool, &config, publisher.clone())
        .with_operator(runtime(&provider, 4));
    (crm_api::build_app(state), provider, publisher)
}

fn call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: "c".to_string(),
        name: name.to_string(),
        arguments: args.to_string(),
    }
}

fn tool_step(name: &str, args: Value) -> ScriptedStep {
    ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(name, args)]))
}

fn text_step(text: &str) -> ScriptedStep {
    ScriptedStep::Respond(ChatResponse::text(text))
}

async fn post_turn(router: &Router, cookie: &str, message: &str) -> axum::response::Response {
    crate::common::post_json_with_cookie(
        router,
        "/api/operator/turns",
        cookie,
        json!({ "message": message, "history": [], "context": { "route": "other" } }),
    )
    .await
}

async fn tool_rows(pool: &PgPool, turn_id: Uuid) -> Vec<(i16, String, String, Vec<Uuid>)> {
    sqlx::query_as(
        "SELECT seq, tool_name, outcome, person_ids FROM operator_tool_call
         WHERE turn_id = $1 ORDER BY seq",
    )
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The wire `tool_calls[]` shape stripped of `duration_ms` — real wall-clock
/// timing, never byte-identical across two separate turns even when the
/// outcome is. "Byte-identical" comparisons (D-046, §8.10) mean the shape
/// the model/caller can observe, not the timing.
fn tool_call_shape(tool_calls: &Value) -> Value {
    Value::Array(
        tool_calls
            .as_array()
            .unwrap()
            .iter()
            .map(|c| json!({ "name": c["name"], "outcome": c["outcome"] }))
            .collect(),
    )
}

async fn proposal_row_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM operator_proposal")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every string in the JSON the provider received, concatenated — for
/// "no foreign/unresolved text leaked" assertions.
fn requests_json(provider: &ScriptedProvider) -> String {
    serde_json::to_string(&provider.requests()).unwrap()
}

/// The last tool-result message's JSON `result` object of the request at
/// `index` (1-based round, matching `provider.requests()[index]`).
fn tool_result(provider: &ScriptedProvider, index: usize) -> Value {
    let msgs = &provider.requests()[index].messages;
    let content = match msgs.last() {
        Some(ChatMessage::Tool { content, .. }) => content,
        other => panic!("expected a tool message, got {other:?}"),
    };
    let parsed: Value = serde_json::from_str(content).unwrap();
    assert_eq!(parsed["ok"], true, "{parsed}");
    parsed["result"].clone()
}

// --- §8.4: a composed filter returns cards, description, count; the
// ledger's person_ids match and tool_name is never "unknown" -----------

#[sqlx::test]
#[ignore]
async fn composed_filter_matches_stage_tag_and_assignee_and_ledger_reflects_it(
    migrator_pool: PgPool,
) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let tag_id = create_tag(&app_pool, f.org, f.alice_id, "Investor").await;
    let grace = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Grace",
        "Hopper",
        Some(f.bob_id),
    )
    .await;
    apply_tag(&app_pool, f.org, f.alice_id, grace, tag_id).await;
    // A decoy that matches the stage but not the tag or assignee.
    insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Ada",
        "Lovelace",
        None,
    )
    .await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "filter_people",
                json!({"stage_names": ["Lead"], "tag_names_any": ["Investor"], "assignees": ["Bob"]}),
            ),
            text_step("Grace Hopper matches."),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    let response = post_turn(&router, &cookie, "Who are my tagged Investors in Lead?").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = crate::common::body_json(response).await;
    let turn_id: Uuid = body["turn_id"].as_str().unwrap().parse().unwrap();
    let people = body["references"]["people"].as_array().unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0]["id"], grace.to_string());

    let result = tool_result(&provider, 1);
    assert_eq!(result["status"], "matched");
    assert_eq!(result["count"], 1);
    assert_eq!(result["more_than_500"], false);
    assert_eq!(result["returned"], 1);
    assert!(result["description"].as_array().unwrap().len() >= 2);
    assert_eq!(result["matches"][0]["id"], grace.to_string());

    let tools = tool_rows(&app_pool, turn_id).await;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].1, "filter_people", "tool_name is never unknown");
    assert_eq!(tools[0].2, "ok");
    assert_eq!(tools[0].3, vec![grace], "ledger person_ids match the cards");
}

/// The SAME vocabulary word ("Investor", "Lead") names independent tags/
/// stages in two different Organizations; each Organization has its own
/// Person carrying it. Org A's resolver must resolve strictly against org
/// A's own rows — org B's Person must never appear in the match, the
/// count, or (as a name or an id) in anything the model saw.
#[sqlx::test]
#[ignore]
async fn same_name_vocabulary_across_two_organizations_stays_isolated(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let tag_a = create_tag(&app_pool, f.org, f.alice_id, "Investor").await;
    let person_a = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Alpha",
        "Investor",
        None,
    )
    .await;
    apply_tag(&app_pool, f.org, f.alice_id, person_a, tag_a).await;

    let (org_b, alice_b) = crate::common::create_org_with_stages_and_member(
        &f.migrator_pool,
        "Beta Realty",
        "alice@beta.test",
        "Alice Beta",
        PW,
    )
    .await;
    let stage_b_lead = first_stage_id(&f.migrator_pool, org_b, "Lead").await;
    let tag_b = create_tag(&app_pool, org_b, alice_b, "Investor").await;
    let person_b = insert_named_person(
        &f.migrator_pool,
        org_b,
        stage_b_lead,
        "Beta",
        "Investor",
        None,
    )
    .await;
    apply_tag(&app_pool, org_b, alice_b, person_b, tag_b).await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "filter_people",
                json!({"tag_names_any": ["Investor"], "stage_names": ["Lead"]}),
            ),
            text_step("Found Alpha Investor."),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let response = post_turn(&router, &cookie, "Show me Investors in Lead").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let result = tool_result(&provider, 1);
    assert_eq!(result["status"], "matched");
    assert_eq!(result["count"], 1);
    assert_eq!(result["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result["matches"][0]["id"], person_a.to_string());

    let prompt = requests_json(&provider);
    assert!(
        !prompt.contains(&person_b.to_string()),
        "org B's Person id never reached the model"
    );
}

// --- §8.5: unknown tag name -> needs_clarification (an Ok outcome that
// does not end the turn, even two in a row) -----------------------------

#[sqlx::test]
#[ignore]
async fn unknown_tag_name_is_needs_clarification_and_two_in_a_row_does_not_end_the_turn(
    migrator_pool: PgPool,
) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    create_tag(&app_pool, f.org, f.alice_id, "Investor").await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"tag_names_any": ["Bogus1"]})),
            tool_step("filter_people", json!({"tag_names_any": ["Bogus2"]})),
            text_step("I couldn't find those tags — which did you mean?"),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    let response = post_turn(&router, &cookie, "Show me Bogus1 people").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["outcome"], "completed");
    let turn_id: Uuid = body["turn_id"].as_str().unwrap().parse().unwrap();

    let first = tool_result(&provider, 1);
    assert_eq!(first["status"], "needs_clarification");
    assert_eq!(first["unknown_tags"], json!(["Bogus1"]));
    assert_eq!(
        first["available_tags"],
        json!([{"untrusted_text": "Investor"}])
    );

    let second = tool_result(&provider, 2);
    assert_eq!(second["status"], "needs_clarification");
    assert_eq!(second["unknown_tags"], json!(["Bogus2"]));

    let tools = tool_rows(&app_pool, turn_id).await;
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].2, "ok", "a clarification is an Ok outcome");
    assert_eq!(tools[1].2, "ok");
}

// --- §8.6: ambiguous member display name; `me` resolves per actor -----

#[sqlx::test]
#[ignore]
async fn ambiguous_assignee_reports_candidates_and_me_resolves_per_actor(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let sam1 = crate::common::create_user(&f.migrator_pool, "sam1@acme.test", "Sam", PW).await;
    crate::common::add_membership_with(
        &f.migrator_pool,
        f.org,
        sam1,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let sam2 = crate::common::create_user(&f.migrator_pool, "sam2@acme.test", "sam", PW).await;
    crate::common::add_membership_with(
        &f.migrator_pool,
        f.org,
        sam2,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"assignees": ["SAM"]})),
            text_step("Which Sam?"),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let response = post_turn(&router, &cookie, "Show me Sam's people").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let result = tool_result(&provider, 1);
    assert_eq!(result["status"], "needs_clarification");
    assert_eq!(result["ambiguous_assignees"], json!(["SAM"]));
    assert!(result["unknown_assignees"].as_array().unwrap().is_empty());
    let members: Vec<String> = result["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(members.contains(&"Sam".to_string()));
    assert!(members.contains(&"sam".to_string()));

    // `me` resolves per actor: Alice's own Person vs Bob's own Person.
    let alice_person = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Alice's",
        "Lead",
        Some(f.alice_id),
    )
    .await;
    let bob_person = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Bob's",
        "Lead",
        Some(f.bob_id),
    )
    .await;

    let (router2, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"assignees": ["me"]})),
            text_step("ok"),
            tool_step("filter_people", json!({"assignees": ["me"]})),
            text_step("ok"),
        ],
    )
    .await;
    let alice_cookie = crate::common::login_cookie(&router2, "alice@acme.test", PW).await;
    let alice_resp = post_turn(&router2, &alice_cookie, "my people").await;
    let alice_body = crate::common::body_json(alice_resp).await;
    assert_eq!(
        alice_body["references"]["people"][0]["id"],
        alice_person.to_string()
    );

    let bob_cookie = crate::common::login_cookie(&router2, "bob@acme.test", PW).await;
    let bob_resp = post_turn(&router2, &bob_cookie, "my people").await;
    let bob_body = crate::common::body_json(bob_resp).await;
    assert_eq!(
        bob_body["references"]["people"][0]["id"],
        bob_person.to_string()
    );
    assert_ne!(
        alice_body["references"]["people"],
        bob_body["references"]["people"]
    );
}

// --- §8.7: last_contact not_within_days includes the never-contacted --

#[sqlx::test]
#[ignore]
async fn last_contact_not_within_days_includes_never_contacted(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let never = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Never",
        "Contacted",
        None,
    )
    .await;
    let stale = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Stale",
        "Contact",
        None,
    )
    .await;
    insert_contact_attempt(
        &f.migrator_pool,
        f.org,
        stale,
        chrono::Utc::now() - chrono::Duration::days(40),
    )
    .await;
    let fresh = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Fresh",
        "Contact",
        None,
    )
    .await;
    insert_contact_attempt(
        &f.migrator_pool,
        f.org,
        fresh,
        chrono::Utc::now() - chrono::Duration::days(5),
    )
    .await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "filter_people",
                json!({"last_contact": {"op": "not_within_days", "days": 30}}),
            ),
            text_step("Here they are, including some never contacted."),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let response = post_turn(&router, &cookie, "Who haven't I contacted in 30 days?").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let result = tool_result(&provider, 1);
    let ids: Vec<String> = result["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&never.to_string()), "never-contacted included");
    assert!(ids.contains(&stale.to_string()));
    assert!(!ids.contains(&fresh.to_string()));
}

// --- §8.8: caps -----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn caps_thirty_matches_with_limit_ten_and_five_hundred_one_matches(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    insert_bare_people_batch(&f.migrator_pool, f.org, f.stage_lead, 30).await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "filter_people",
                json!({"stage_names": ["Lead"], "limit": 10}),
            ),
            text_step("Here are 10 of your 30 Leads."),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let response = post_turn(&router, &cookie, "Show me my Leads, 10 at a time").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let result = tool_result(&provider, 1);
    assert_eq!(result["count"], 30);
    assert_eq!(result["returned"], 10);
    assert_eq!(result["matches"].as_array().unwrap().len(), 10);
    assert_eq!(result["more_than_500"], false);

    // A fresh org for the 501 case so this test's own fixture stays isolated.
    let (org2, alice2) = crate::common::create_org_with_stages_and_member(
        &f.migrator_pool,
        "Acme F13",
        "alice2@acmef13.test",
        "Alice",
        PW,
    )
    .await;
    let stage2 = first_stage_id(&f.migrator_pool, org2, "Lead").await;
    insert_bare_people_batch(&f.migrator_pool, org2, stage2, 501).await;
    let (router2, provider2) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"stage_names": ["Lead"]})),
            text_step("There are more than 500."),
        ],
    )
    .await;
    let cookie2 = crate::common::login_cookie(&router2, "alice2@acmef13.test", PW).await;
    let _ = alice2;
    let response2 = post_turn(&router2, &cookie2, "How many Leads do I have?").await;
    assert_eq!(response2.status(), axum::http::StatusCode::OK);
    let result2 = tool_result(&provider2, 1);
    assert_eq!(result2["count"], 500);
    assert_eq!(result2["more_than_500"], true);
}

// --- §8.9: run_saved_list, D-046 visibility matrix ---------------------

#[sqlx::test]
#[ignore]
async fn own_personal_list_and_shared_list_are_visible_to_member_and_admin(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let bob_person = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Bob's",
        "Lead",
        Some(f.bob_id),
    )
    .await;
    create_list(
        &app_pool,
        f.org,
        f.bob_id,
        SavedListScope::Personal,
        "My Own List",
        vec![crm_api::domain::person::filter::Clause::AssignedTo(
            crm_api::domain::person::filter::AssignedToClause {
                assignees: vec![crm_api::domain::person::filter::Assignee::User(
                    UserId::new(f.bob_id),
                )],
            },
        )],
        None,
    )
    .await;
    let stage_lead = f.stage_lead;
    let all_lead =
        insert_named_person(&f.migrator_pool, f.org, stage_lead, "Shared", "Match", None).await;
    create_list(
        &app_pool,
        f.org,
        f.alice_id,
        SavedListScope::Shared,
        "Team Leads",
        vec![crm_api::domain::person::filter::Clause::Stage(
            crm_api::domain::person::filter::StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(stage_lead)],
            },
        )],
        None,
    )
    .await;

    // Bob sees his own personal list.
    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name": "My Own List"})),
            text_step("ok"),
        ],
    )
    .await;
    let bob_cookie = crate::common::login_cookie(&router, "bob@acme.test", PW).await;
    let resp = post_turn(&router, &bob_cookie, "run my own list").await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let result = tool_result(&provider, 1);
    assert_eq!(result["status"], "matched");
    assert_eq!(result["matches"][0]["id"], bob_person.to_string());
    assert_eq!(result["list"]["scope"], "personal");

    // Both Bob and Alice (admin) see the shared list.
    for (email, expected_scope) in [("bob@acme.test", "shared"), ("alice@acme.test", "shared")] {
        let (router, provider) = router_scripted(
            &f.migrator_pool,
            vec![
                tool_step("run_saved_list", json!({"name": "Team Leads"})),
                text_step("ok"),
            ],
        )
        .await;
        let cookie = crate::common::login_cookie(&router, email, PW).await;
        let resp = post_turn(&router, &cookie, "run team leads").await;
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let result = tool_result(&provider, 1);
        assert_eq!(result["status"], "matched", "{email}");
        assert_eq!(result["list"]["scope"], expected_scope);
        let ids: Vec<String> = result["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect();
        assert!(ids.contains(&all_lead.to_string()), "{email}");
    }
}

#[sqlx::test]
#[ignore]
async fn another_members_personal_list_is_not_found_including_for_admin(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let bobs_list_id = create_list(
        &app_pool,
        f.org,
        f.bob_id,
        SavedListScope::Personal,
        "Bob's Secret List",
        vec![crm_api::domain::person::filter::Clause::Stage(
            crm_api::domain::person::filter::StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(f.stage_lead)],
            },
        )],
        None,
    )
    .await;

    // Not found for the creator's admin colleague, byte-identical to a
    // wholly nonexistent name (D-046 rule 1: no admin exception).
    let (router_a, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name": "Bob's Secret List"})),
            text_step("ok"),
        ],
    )
    .await;
    let alice_cookie = crate::common::login_cookie(&router_a, "alice@acme.test", PW).await;
    let resp_a = post_turn(&router_a, &alice_cookie, "run bob's secret list").await;
    let body_a = crate::common::body_json(resp_a).await;

    let (router_b, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name": "Totally Nonexistent"})),
            text_step("ok"),
        ],
    )
    .await;
    let alice_cookie2 = crate::common::login_cookie(&router_b, "alice@acme.test", PW).await;
    let resp_b = post_turn(&router_b, &alice_cookie2, "run a nonexistent list").await;
    let body_b = crate::common::body_json(resp_b).await;

    assert_eq!(body_a["outcome"], body_b["outcome"]);
    assert_eq!(body_a["tool_calls"][0]["outcome"], "not_found");
    assert_eq!(body_b["tool_calls"][0]["outcome"], "not_found");

    // The id path, not only the name path: an admin passing another
    // member's personal list_id DIRECTLY is byte-identical to a random
    // uuid — `saved_list_detail`'s own visibility predicate refuses it
    // regardless of how the model got the id.
    let (router_c, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "run_saved_list",
                json!({"list_id": bobs_list_id.as_uuid().to_string()}),
            ),
            text_step("ok"),
        ],
    )
    .await;
    let alice_cookie3 = crate::common::login_cookie(&router_c, "alice@acme.test", PW).await;
    let resp_c = post_turn(&router_c, &alice_cookie3, "run that list id").await;
    let body_c = crate::common::body_json(resp_c).await;

    let (router_d, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "run_saved_list",
                json!({"list_id": Uuid::new_v4().to_string()}),
            ),
            text_step("ok"),
        ],
    )
    .await;
    let alice_cookie4 = crate::common::login_cookie(&router_d, "alice@acme.test", PW).await;
    let resp_d = post_turn(&router_d, &alice_cookie4, "run a random list id").await;
    let body_d = crate::common::body_json(resp_d).await;

    assert_eq!(body_c["outcome"], body_d["outcome"]);
    assert_eq!(
        tool_call_shape(&body_c["tool_calls"]),
        tool_call_shape(&body_d["tool_calls"]),
        "byte-identical"
    );
    assert_eq!(body_c["tool_calls"][0]["outcome"], "not_found");
}

#[sqlx::test]
#[ignore]
async fn duplicate_named_lists_report_candidates_then_list_id_resolves(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let shared_id = create_list(
        &app_pool,
        f.org,
        f.alice_id,
        SavedListScope::Shared,
        "Weekly",
        vec![crm_api::domain::person::filter::Clause::Stage(
            crm_api::domain::person::filter::StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(f.stage_lead)],
            },
        )],
        None,
    )
    .await;
    let personal_id = create_list(
        &app_pool,
        f.org,
        f.bob_id,
        SavedListScope::Personal,
        "Weekly",
        vec![crm_api::domain::person::filter::Clause::Stage(
            crm_api::domain::person::filter::StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(f.stage_lead)],
            },
        )],
        None,
    )
    .await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name": "Weekly"})),
            tool_step(
                "run_saved_list",
                json!({"list_id": personal_id.as_uuid().to_string()}),
            ),
            text_step("ok"),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "bob@acme.test", PW).await;
    let resp = post_turn(&router, &cookie, "run Weekly").await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    let first = tool_result(&provider, 1);
    assert_eq!(first["status"], "needs_clarification");
    let candidates = first["candidate_lists"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    let candidate_ids: Vec<String> = candidates
        .iter()
        .map(|c| c["list_id"].as_str().unwrap().to_string())
        .collect();
    assert!(candidate_ids.contains(&shared_id.as_uuid().to_string()));
    assert!(candidate_ids.contains(&personal_id.as_uuid().to_string()));

    let second = tool_result(&provider, 2);
    assert_eq!(second["status"], "matched");
    assert_eq!(second["list"]["list_id"], personal_id.as_uuid().to_string());
}

#[sqlx::test]
#[ignore]
async fn a_list_with_invalid_tag_is_list_invalid(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let tag_id = create_tag(&app_pool, f.org, f.alice_id, "Stale").await;
    create_list(
        &app_pool,
        f.org,
        f.alice_id,
        SavedListScope::Shared,
        "Stale Tag List",
        vec![crm_api::domain::person::filter::Clause::Tags(
            crm_api::domain::person::filter::TagIdsClause {
                tag_ids: vec![tag_id],
            },
        )],
        None,
    )
    .await;
    // No Person carries the tag, so the creator can delete it (D-051 item
    // 2), leaving the stored list definition referencing a stale tag id.
    delete_tag(&app_pool, f.org, f.alice_id, tag_id).await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name": "Stale Tag List"})),
            text_step("That list can't be evaluated right now."),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let resp = post_turn(&router, &cookie, "run Stale Tag List").await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let result = tool_result(&provider, 1);
    assert_eq!(result["status"], "list_invalid");
    assert_eq!(result["error"], "invalid_tag");
}

/// Stored custom-filter descriptions are model input and must retain the
/// `UntrustedText` wrapper.  A read-only list run still gets an auditable
/// ledger row, but cannot publish a realtime mutation event or persist the
/// sentinel label outside the wrapped provider result.
#[sqlx::test]
#[ignore]
async fn custom_saved_list_description_is_untrusted_and_read_only(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let sentinel = "SENTINEL_CUSTOM_FILTER_LABEL";
    let field_id: Uuid = sqlx::query_scalar(
        "INSERT INTO custom_field (organization_id, label, field_type, position, created_by_user_id) \
         VALUES ($1, $2, 'text', 1, $3) RETURNING id",
    )
    .bind(f.org)
    .bind(sentinel)
    .bind(f.alice_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    let clause = serde_json::from_value(json!({
        "kind":"custom_text", "field_id":field_id,
        "test":{"op":"contains", "text":"needle"}
    }))
    .unwrap();
    create_list(
        &app_pool,
        f.org,
        f.alice_id,
        SavedListScope::Personal,
        "Custom Operator List",
        vec![clause],
        None,
    )
    .await;
    let (router, provider, publisher) = router_scripted_with_publisher(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name":"Custom Operator List"})),
            text_step("ok"),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let response = post_turn(&router, &cookie, "run my custom list").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = crate::common::body_json(response).await;
    let result = tool_result(&provider, 1);
    assert_eq!(result["status"], "matched");
    assert!(result["description"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| line
            .get("untrusted_text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains(sentinel))));
    let ledger = tool_rows(
        &app_pool,
        Uuid::parse_str(body["turn_id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger[0].1, "run_saved_list");
    assert_eq!(ledger[0].2, "ok");
    let Publisher::Recording(events, _) = publisher else {
        panic!("expected recording publisher");
    };
    assert!(
        events.lock().await.is_empty(),
        "a read-only list run must not publish realtime events"
    );
    let ledger_text: String = sqlx::query_scalar(
        "SELECT coalesce(string_agg(tool_name || ':' || outcome, ','), '') FROM operator_tool_call",
    )
    .fetch_one(&app_pool)
    .await
    .unwrap();
    assert!(!ledger_text.contains(sentinel));
}

#[sqlx::test]
#[ignore]
async fn run_saved_list_honours_the_stored_sort(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    // Inserted in an order that is NOT alphabetical, so a Name-ascending
    // sort is the only way the response could come back alphabetized.
    insert_named_person(&f.migrator_pool, f.org, f.stage_lead, "Zed", "Zephyr", None).await;
    insert_named_person(&f.migrator_pool, f.org, f.stage_lead, "Amy", "Adams", None).await;
    insert_named_person(&f.migrator_pool, f.org, f.stage_lead, "Mel", "Moss", None).await;
    create_list(
        &app_pool,
        f.org,
        f.alice_id,
        SavedListScope::Shared,
        "Alphabetical Leads",
        vec![crm_api::domain::person::filter::Clause::Stage(
            crm_api::domain::person::filter::StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(f.stage_lead)],
            },
        )],
        Some(PersonSort {
            key: SortKey::Name,
            direction: SortDirection::Asc,
        }),
    )
    .await;

    let (router, provider) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("run_saved_list", json!({"name": "Alphabetical Leads"})),
            text_step("ok"),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let resp = post_turn(&router, &cookie, "run Alphabetical Leads").await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let result = tool_result(&provider, 1);
    let names: Vec<String> = result["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["display_name"]["untrusted_text"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(names, vec!["Amy Adams", "Mel Moss", "Zed Zephyr"]);
}

// --- §8.10: foreign list_id / foreign names are byte-identical to
// nonexistent; no foreign name appears in any prompt --------------------

#[sqlx::test]
#[ignore]
async fn foreign_list_id_and_foreign_tag_name_are_byte_identical_to_nonexistent(
    migrator_pool: PgPool,
) {
    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;

    let (org_b, alice_b) = crate::common::create_org_with_stages_and_member(
        &f.migrator_pool,
        "Best Realty",
        "alice@best.test",
        "Alice B",
        PW,
    )
    .await;
    let foreign_tag_id = create_tag(&app_pool, org_b, alice_b, "OnlyInOrgB").await;
    let _ = foreign_tag_id;
    // Personal, not Shared: `create_org_with_stages_and_member` leaves
    // `alice_b` a plain member, and only an admin may create a shared
    // list — irrelevant to this test, which only needs A list foreign to
    // org A.
    let foreign_list_id = create_list(
        &app_pool,
        org_b,
        alice_b,
        SavedListScope::Personal,
        "Best Realty's Only List",
        vec![crm_api::domain::person::filter::Clause::HasPhone(
            crm_api::domain::person::filter::BoolClause { value: true },
        )],
        None,
    )
    .await;

    // Foreign list_id vs a random nonexistent one: same not_found shape.
    let (router1, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "run_saved_list",
                json!({"list_id": foreign_list_id.as_uuid().to_string()}),
            ),
            text_step("ok"),
        ],
    )
    .await;
    let cookie1 = crate::common::login_cookie(&router1, "alice@acme.test", PW).await;
    let resp1 = post_turn(&router1, &cookie1, "run that foreign list").await;
    let body1 = crate::common::body_json(resp1).await;

    let (router2, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "run_saved_list",
                json!({"list_id": Uuid::new_v4().to_string()}),
            ),
            text_step("ok"),
        ],
    )
    .await;
    let cookie2 = crate::common::login_cookie(&router2, "alice@acme.test", PW).await;
    let resp2 = post_turn(&router2, &cookie2, "run a made-up list").await;
    let body2 = crate::common::body_json(resp2).await;

    assert_eq!(body1["tool_calls"][0]["outcome"], "not_found");
    assert_eq!(
        tool_call_shape(&body1["tool_calls"]),
        tool_call_shape(&body2["tool_calls"])
    );
    assert!(
        !requests_json(&provider(vec![])).contains("Best Realty's Only List"),
        "sanity: helper compiles"
    );

    // Foreign tag name vs a nonexistent one: byte-identical clarification
    // shape, and the caller's own (empty) tag vocabulary only — never a
    // hint that the name exists in another Organization.
    let (router3, provider3) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"tag_names_any": ["OnlyInOrgB"]})),
            text_step("ok"),
        ],
    )
    .await;
    let cookie3 = crate::common::login_cookie(&router3, "alice@acme.test", PW).await;
    let resp3 = post_turn(&router3, &cookie3, "show me OnlyInOrgB people").await;
    assert_eq!(resp3.status(), axum::http::StatusCode::OK);
    let result3 = tool_result(&provider3, 1);

    let (router4, provider4) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"tag_names_any": ["TotallyMadeUp"]})),
            text_step("ok"),
        ],
    )
    .await;
    let cookie4 = crate::common::login_cookie(&router4, "alice@acme.test", PW).await;
    let resp4 = post_turn(&router4, &cookie4, "show me TotallyMadeUp people").await;
    assert_eq!(resp4.status(), axum::http::StatusCode::OK);
    let result4 = tool_result(&provider4, 1);

    assert_eq!(result3["status"], result4["status"]);
    assert_eq!(result3["status"], "needs_clarification");
    assert_eq!(result3["available_tags"], result4["available_tags"]);
    assert_eq!(
        result3["available_tags"],
        json!([]),
        "org A has no tags of its own"
    );
    assert!(
        !requests_json(&provider3).contains("Best Realty's Only List"),
        "no foreign list name leaked into any prompt"
    );
}

// --- §8.11: a full scripted turn returns 200 with references from the
// filter and writes no operator_proposal row -----------------------------

#[sqlx::test]
#[ignore]
async fn full_turn_returns_200_with_references_and_no_proposal_row(migrator_pool: PgPool) {
    let f = fixture(migrator_pool).await;
    let person = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Grace",
        "Hopper",
        None,
    )
    .await;

    let (router, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step("filter_people", json!({"stage_names": ["Lead"]})),
            text_step("Here is Grace Hopper."),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;
    let resp = post_turn(&router, &cookie, "Who's in Lead?").await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = crate::common::body_json(resp).await;
    assert_eq!(body["outcome"], "completed");
    assert_eq!(body["references"]["people"][0]["id"], person.to_string());

    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    assert_eq!(
        proposal_row_count(&app_pool).await,
        0,
        "filter_people/run_saved_list are read-only: no proposal row"
    );
}

// --- §8.13: span/log capture contains filter_kinds and no names, ids,
// or day counts ----------------------------------------------------------

#[sqlx::test]
#[ignore]
async fn span_capture_contains_filter_kinds_and_no_names_ids_or_day_counts(migrator_pool: PgPool) {
    use std::sync::Mutex;
    use tracing_subscriber::layer::SubscriberExt;

    let f = fixture(migrator_pool).await;
    let app_pool = crate::common::connect_as_app(&f.migrator_pool).await;
    let tag_id = create_tag(&app_pool, f.org, f.alice_id, "Investor").await;
    let person = insert_named_person(
        &f.migrator_pool,
        f.org,
        f.stage_lead,
        "Grace",
        "Hopper",
        None,
    )
    .await;
    apply_tag(&app_pool, f.org, f.alice_id, person, tag_id).await;
    let list_id = create_list(
        &app_pool,
        f.org,
        f.alice_id,
        SavedListScope::Shared,
        "Distinctive List Name",
        vec![crm_api::domain::person::filter::Clause::Stage(
            crm_api::domain::person::filter::StageClause {
                stage_ids: vec![crm_api::ids::StageId::new(f.stage_lead)],
            },
        )],
        None,
    )
    .await;

    let (router, _) = router_scripted(
        &f.migrator_pool,
        vec![
            tool_step(
                "filter_people",
                json!({"tag_names_any": ["Investor"], "last_contact": {"op": "not_within_days", "days": 47}}),
            ),
            tool_step("run_saved_list", json!({"name": "Distinctive List Name"})),
            text_step("ok"),
        ],
    )
    .await;
    let cookie = crate::common::login_cookie(&router, "alice@acme.test", PW).await;

    #[derive(Clone, Default)]
    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
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
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    // nextest isolates every test in its own process (docs/tasks/
    // SLICE_013_IMPL.md; the same pattern as db_operator_call.rs's
    // capture test), so a process-global subscriber here does not
    // conflict with that or any other test's install.
    tracing::subscriber::set_global_default(subscriber)
        .expect("the log-capture test must be the only one installing a subscriber");

    let resp = post_turn(
        &router,
        &cookie,
        "Investors not contacted in 47 days, then run my list",
    )
    .await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);

    let captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(!captured.is_empty());
    // filter_people's clauses are built last_contact, then tags (clause
    // order in operator::filter::resolve — last_contact is bound before
    // the trailing tags/not_tags block), so `kinds_field()` joins them in
    // that order: the exact string this turn's filter_people call binds.
    assert!(
        captured.contains(r#"filter_kinds="last_contact,tags""#),
        "{captured}"
    );
    assert!(captured.contains("match_count=1"), "{captured}");
    assert!(captured.contains(r#"resolution="matched""#), "{captured}");
    assert!(
        captured.contains(r#"saved_list_scope="shared""#),
        "{captured}"
    );
    for leaked in ["Investor", "Grace", "Hopper", "Distinctive List Name"] {
        assert!(
            !captured.contains(leaked),
            "leaked into spans/logs: {leaked}"
        );
    }
    // Ids are D-029-forbidden exactly like names: the Person, the tag, and
    // the saved list's own uuid must never appear either.
    for leaked_id in [
        person.to_string(),
        tag_id.to_string(),
        list_id.as_uuid().to_string(),
    ] {
        assert!(
            !captured.contains(&leaked_id),
            "an id leaked into spans/logs: {leaked_id}"
        );
    }
    // Day counts: no span here declares or records a "days" field at all,
    // so the direct test is that no such field-assignment ever appears —
    // asserting the bare digits of the day count itself is unreliable
    // (they can coincidentally appear inside an unrelated hex UUID
    // elsewhere in a capture this size).
    assert!(
        !captured.contains("days="),
        "a day-count field leaked into spans/logs"
    );
}

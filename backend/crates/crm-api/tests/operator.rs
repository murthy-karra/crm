//! Service-free HTTP tests for `POST /api/operator/turns`
//! (docs/specs/SLICE_005.md §13 item 3). Every other case needs a session,
//! which `AuthContext` resolves against the database — same split as
//! `tests/today.rs`.
use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crm_api::config::Config;
use crm_api::state::AppState;

fn test_config(overrides: &[(&str, &str)]) -> Config {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert("CRM_SESSION_SECRET".to_string(), "a".repeat(32));
    map.insert("CRM_RAW_PAYLOAD_KEY".to_string(), "ab".repeat(32));
    map.insert(
        "CENTRIFUGO_HTTP_API_KEY".to_string(),
        "test-centrifugo-api-key".to_string(),
    );
    map.insert("CENTRIFUGO_TOKEN_HMAC_SECRET".to_string(), "c".repeat(32));
    for (k, v) in overrides {
        map.insert((*k).to_string(), (*v).to_string());
    }
    Config::from_source(move |key| map.get(key).cloned()).expect("valid test config")
}

fn unreachable_database_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("postgres://user:pass@{addr}/db")
}

fn turn_request(cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/operator/turns")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    builder
        .body(Body::from(
            serde_json::json!({ "message": "Who should I call next?" }).to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn post_turn_without_cookie_returns_401() {
    let state = AppState::new(&test_config(&[])).unwrap();
    let app = crm_api::build_app(state);
    let response = app.oneshot(turn_request(None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_turn_returns_503_unavailable_when_database_unreachable() {
    let database_url = unreachable_database_url();
    let state = AppState::new(&test_config(&[
        ("DATABASE_URL", database_url.as_str()),
        ("CRM_DATABASE_CONNECT_TIMEOUT_MS", "200"),
        ("GROQ_API_KEY", "gsk_test_key_never_used"),
    ]))
    .unwrap();
    assert!(state.operator.is_some(), "a key enables the runtime");
    let app = crm_api::build_app(state);
    let plausible_token = "a".repeat(43);
    let cookie = format!("crm_session={plausible_token}");

    let start = Instant::now();
    let response = app.oneshot(turn_request(Some(&cookie))).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "did not respect the timeout bound: {elapsed:?}"
    );
}

#[tokio::test]
async fn runtime_is_absent_without_a_key() {
    let state = AppState::new(&test_config(&[])).unwrap();
    assert!(state.operator.is_none());
}

// --- filter_people schema mirrors every Clause kind (docs/specs/
// SLICE_013.md §2, §8.3) ----------------------------------------------------
//
// `crm-operator` cannot depend on `crm-app` (D-034,
// crm-api/tests/operator_deps.rs), so the test that walks the REAL
// `crm_app::domain::person::filter::Clause::kind_label()` values against
// crm-operator's `filter_people` JSON schema has to live here, on the
// crm-api side of the fence, where both crates are already dependencies.
// `crm-operator/src/tools.rs` also carries its own
// `filter_people_schema_declares_a_field_for_every_mirrored_clause` test
// against a hardcoded copy of this same 15-entry list, so a missing schema
// field fails fast in that crate too; this test is what actually proves the
// hardcoded list matches the live `Clause` enum, not just crm-operator's
// own idea of it.
mod filter_people_mirrors_every_clause_kind {
    use crm_app::domain::person::filter::{
        AgeClause, AgeSpec, AssignedToClause, BoolClause, Clause, SourceClause, StageClause,
        TagIdsClause,
    };
    use crm_app::ids::{StageId, TagId};

    /// One instance of every `Clause` variant, with placeholder field
    /// values — only `kind_label()` is exercised, never the payload.
    fn one_of_every_clause() -> Vec<Clause> {
        let stage_id = StageId::new(uuid::Uuid::nil());
        let tag_id = TagId::new(uuid::Uuid::nil());
        let age = AgeClause {
            age: AgeSpec::WithinDays(1),
        };
        vec![
            Clause::Stage(StageClause {
                stage_ids: vec![stage_id],
            }),
            Clause::AssignedTo(AssignedToClause {
                assignees: vec![crm_app::domain::person::filter::Assignee::Me],
            }),
            Clause::Source(SourceClause {
                sources: vec!["zillow".to_string()],
            }),
            Clause::Created(age),
            Clause::LastInquiry(age),
            Clause::LastContact(age),
            Clause::LastInbound(age),
            Clause::HasReplied(BoolClause { value: true }),
            Clause::HasPhone(BoolClause { value: true }),
            Clause::HasEmail(BoolClause { value: true }),
            Clause::AwaitingResponse(BoolClause { value: true }),
            Clause::ClientRepliedUnanswered(BoolClause { value: true }),
            Clause::AwaitingCallOutcome(BoolClause { value: true }),
            Clause::Tags(TagIdsClause {
                tag_ids: vec![tag_id],
            }),
            Clause::NotTags(TagIdsClause {
                tag_ids: vec![tag_id],
            }),
        ]
    }

    /// docs/specs/SLICE_013.md §2's table: every `Clause::kind_label()` to
    /// the `filter_people` schema property name it mirrors. Ids become
    /// names (`stage`→`stage_names`, `assigned_to`→`assignees`,
    /// `source`→`sources`, `tags`/`not_tags`→`tag_names_any`/
    /// `tag_names_none`); every other kind keeps its own name.
    fn expected_field_for_kind(kind: &str) -> &'static str {
        match kind {
            "stage" => "stage_names",
            "assigned_to" => "assignees",
            "source" => "sources",
            "created" => "created",
            "last_inquiry" => "last_inquiry",
            "last_contact" => "last_contact",
            "last_inbound" => "last_inbound",
            "has_replied" => "has_replied",
            "has_phone" => "has_phone",
            "has_email" => "has_email",
            "awaiting_response" => "awaiting_response",
            "client_replied_unanswered" => "client_replied_unanswered",
            "awaiting_call_outcome" => "awaiting_call_outcome",
            "tags" => "tag_names_any",
            "not_tags" => "tag_names_none",
            other => panic!("unmapped Clause::kind_label(): {other}"),
        }
    }

    #[test]
    fn every_clause_kind_has_a_filter_people_schema_field() {
        let def = crm_operator::tool_definitions()
            .into_iter()
            .find(|d| d.name == "filter_people")
            .expect("filter_people is declared");
        let props = def.parameters["properties"]
            .as_object()
            .expect("filter_people has an object schema");

        let mut seen_kinds = std::collections::HashSet::new();
        for clause in one_of_every_clause() {
            let kind = clause.kind_label();
            assert!(seen_kinds.insert(kind), "duplicate fixture for {kind}");
            let field = expected_field_for_kind(kind);
            assert!(
                props.contains_key(field),
                "Clause::kind_label() {kind:?} has no filter_people schema field {field:?}"
            );
        }
        // And nothing on the other side: every kind_label this test knows
        // about was actually produced by the fixture above (protects
        // against a stale mapping entry no live Clause variant reaches).
        assert_eq!(seen_kinds.len(), 15, "expected all fifteen Clause kinds");
    }
}

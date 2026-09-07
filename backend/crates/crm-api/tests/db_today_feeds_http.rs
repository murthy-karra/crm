//! Review round 1, F4: table-driven HTTP wire tests for the system-feed
//! surface (docs/specs/SLICE_011d.md §6). Admin, member and platform-only
//! sessions walking every declared shape, error precedence and envelope.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crm_api::domain::admin::{MembershipStatus, Role};

const PW: &str = "correct horse battery staple";

async fn create_org_with_admin_and_member(
    pool: &PgPool,
    org_name: &str,
    admin_email: &str,
    member_email: &str,
) -> (Uuid, Uuid, Uuid) {
    let org_id = crate::common::create_org(pool, org_name).await;
    crate::common::seed_stages(pool, org_id).await;
    let admin_id = crate::common::create_user(pool, admin_email, "Admin", PW).await;
    crate::common::add_membership_with(
        pool,
        org_id,
        admin_id,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let member_id = crate::common::create_user(pool, member_email, "Member", PW).await;
    crate::common::add_membership_with(
        pool,
        org_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    (org_id, admin_id, member_id)
}

fn canonical_body() -> Value {
    json!({
        "expected_revision": 1,
        "filter": {
            "version": 1,
            "clauses": [
                {"kind": "assigned_to", "assignees": ["me"]},
                {"kind": "awaiting_response", "value": true}
            ]
        },
        "fresh_within_hours": 24
    })
}

async fn oversize_put(router: &axum::Router, uri: &str, cookie: &str) -> axum::response::Response {
    // Raw oversize bytes past 128 KiB, mirroring inbound_email.rs's
    // `oversize_body_with_bad_bearer_is_413_with_the_envelope` exactly:
    // the body-SIZE limit must reject this before any JSON parsing or
    // structural validation ever runs, so content need not be valid JSON.
    let oversize = vec![b'x'; 128 * 1024 + 1];
    use tower::ServiceExt;
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(oversize))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[sqlx::test]
#[ignore]
async fn admin_get_returns_all_feed_keys_in_fixed_order(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 admin shape",
        "admin@d011-f4-admin-shape.test",
        "member@d011-f4-admin-shape.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-admin-shape.test", PW).await;

    let response =
        crate::common::get_with_cookie(&router, "/api/organization/today-feeds", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::common::body_json(response).await;
    let feeds = body["feeds"].as_array().unwrap();
    assert_eq!(feeds.len(), 3);
    let keys: Vec<&str> = feeds
        .iter()
        .map(|f| f["feed_key"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        vec![
            "unanswered_inquiry",
            "client_replied",
            "call_outcome_needed"
        ]
    );
    for feed in feeds {
        for key in [
            "feed_key",
            "enabled",
            "revision",
            "is_default",
            "filter",
            "fresh_within_hours",
            "description",
            "filter_error",
            "updated_at",
            "updated_by",
            "default",
        ] {
            assert!(feed.get(key).is_some(), "Feed is missing `{key}`");
        }
    }
}

#[sqlx::test]
#[ignore]
async fn member_get_returns_member_feed_shape(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 member shape",
        "admin@d011-f4-member-shape.test",
        "member@d011-f4-member-shape.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "member@d011-f4-member-shape.test", PW).await;

    let response = crate::common::get_with_cookie(&router, "/api/today/feeds", &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::common::body_json(response).await;
    let feeds = body["feeds"].as_array().unwrap();
    assert_eq!(feeds.len(), 3);
    for feed in feeds {
        let keys: std::collections::BTreeSet<&str> = feed
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(
            keys,
            ["feed_key", "enabled", "is_default", "description"]
                .into_iter()
                .collect()
        );
    }
    // Admin-only fields never leak to the member shape (exact JSON key
    // match, not a substring — `is_default` legitimately contains
    // "default" as a substring but is part of the member shape itself).
    let dump = serde_json::to_string(&body).unwrap();
    for forbidden in [
        "\"revision\"",
        "\"filter_error\"",
        "\"updated_by\"",
        "\"default\"",
    ] {
        assert!(!dump.contains(forbidden), "member shape leaked {forbidden}");
    }
}

#[sqlx::test]
#[ignore]
async fn put_with_an_unknown_body_field_is_400(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 unknown field",
        "admin@d011-f4-unknown-field.test",
        "member@d011-f4-unknown-field.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-unknown-field.test", PW).await;

    let mut body = canonical_body();
    body["unexpected_field"] = json!(true);
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn expected_revision_wire_type_and_range_violations_are_400(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 revision wire",
        "admin@d011-f4-revision-wire.test",
        "member@d011-f4-revision-wire.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-revision-wire.test", PW).await;

    for bad_revision in [json!(0), json!(-1), json!("1"), json!(1.5)] {
        let mut body = canonical_body();
        body["expected_revision"] = bad_revision.clone();
        let response = crate::common::put_json_with_cookie(
            &router,
            "/api/organization/today-feeds/unanswered_inquiry",
            &cookie,
            body,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "expected_revision = {bad_revision}"
        );
    }
}

#[sqlx::test]
#[ignore]
async fn filter_version_2_is_400(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 version",
        "admin@d011-f4-version.test",
        "member@d011-f4-version.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-version.test", PW).await;

    let mut body = canonical_body();
    body["filter"]["version"] = json!(2);
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn unknown_clause_kind_is_400(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 unknown clause",
        "admin@d011-f4-unknown-clause.test",
        "member@d011-f4-unknown-clause.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie =
        crate::common::login_cookie(&router, "admin@d011-f4-unknown-clause.test", PW).await;

    let body = json!({
        "expected_revision": 1,
        "filter": {
            "version": 1,
            "clauses": [{"kind": "not_a_real_clause_kind", "value": true}]
        },
        "fresh_within_hours": 24
    });
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn twenty_one_clauses_is_400(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 clause cap",
        "admin@d011-f4-clause-cap.test",
        "member@d011-f4-clause-cap.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-clause-cap.test", PW).await;

    let mut clauses = vec![
        json!({"kind": "assigned_to", "assignees": ["me"]}),
        json!({"kind": "awaiting_response", "value": true}),
    ];
    for _ in 0..19 {
        clauses.push(json!({"kind": "has_phone", "value": true}));
    }
    assert_eq!(clauses.len(), 21);
    let body = json!({
        "expected_revision": 1,
        "filter": {"version": 1, "clauses": clauses},
        "fresh_within_hours": 24
    });
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
#[ignore]
async fn put_on_an_unknown_feed_key_with_a_valid_body_is_404(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 unknown feed key",
        "admin@d011-f4-unknown-feed-key.test",
        "member@d011-f4-unknown-feed-key.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie =
        crate::common::login_cookie(&router, "admin@d011-f4-unknown-feed-key.test", PW).await;

    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/not_a_real_feed_key",
        &cookie,
        canonical_body(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
#[ignore]
async fn stale_revision_is_409_today_feed_conflict(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 stale revision",
        "admin@d011-f4-stale.test",
        "member@d011-f4-stale.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-stale.test", PW).await;

    let mut body = canonical_body();
    body["expected_revision"] = json!(2);
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "today_feed_conflict");
}

/// F3 (via HTTP): a stale revision AND an invalid stage reference together
/// still return 409, never 422 — the wire-level proof of the same
/// precedence the domain-level test proves directly.
#[sqlx::test]
#[ignore]
async fn stale_revision_plus_invalid_stage_is_409_not_422(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 stale plus invalid",
        "admin@d011-f4-stale-invalid.test",
        "member@d011-f4-stale-invalid.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-stale-invalid.test", PW).await;

    let body = json!({
        "expected_revision": 2,
        "filter": {
            "version": 1,
            "clauses": [
                {"kind": "assigned_to", "assignees": ["me"]},
                {"kind": "awaiting_response", "value": true},
                {"kind": "stage", "stage_ids": [Uuid::new_v4()]}
            ]
        },
        "fresh_within_hours": 24
    });
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "today_feed_conflict");
}

#[sqlx::test]
#[ignore]
async fn feed_rule_violations_are_422_invalid_feed_rule(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 feed rule",
        "admin@d011-f4-feed-rule.test",
        "member@d011-f4-feed-rule.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-feed-rule.test", PW).await;

    let cases: Vec<(&str, Value)> = vec![
        (
            "anchor removed",
            json!({
                "expected_revision": 1,
                "filter": {"version": 1, "clauses": [{"kind": "assigned_to", "assignees": ["me"]}]},
                "fresh_within_hours": 24
            }),
        ),
        (
            "anchor negated",
            json!({
                "expected_revision": 1,
                "filter": {
                    "version": 1,
                    "clauses": [
                        {"kind": "assigned_to", "assignees": ["me"]},
                        {"kind": "awaiting_response", "value": false}
                    ]
                },
                "fresh_within_hours": 24
            }),
        ),
        (
            "assigned_to without me",
            json!({
                "expected_revision": 1,
                "filter": {
                    "version": 1,
                    "clauses": [
                        {"kind": "assigned_to", "assignees": ["unassigned"]},
                        {"kind": "awaiting_response", "value": true}
                    ]
                },
                "fresh_within_hours": 24
            }),
        ),
        (
            "window 0",
            json!({
                "expected_revision": 1,
                "filter": {
                    "version": 1,
                    "clauses": [
                        {"kind": "assigned_to", "assignees": ["me"]},
                        {"kind": "awaiting_response", "value": true}
                    ]
                },
                "fresh_within_hours": 0
            }),
        ),
        (
            "window 8761",
            json!({
                "expected_revision": 1,
                "filter": {
                    "version": 1,
                    "clauses": [
                        {"kind": "assigned_to", "assignees": ["me"]},
                        {"kind": "awaiting_response", "value": true}
                    ]
                },
                "fresh_within_hours": 8761
            }),
        ),
        (
            "window absent",
            json!({
                "expected_revision": 1,
                "filter": {
                    "version": 1,
                    "clauses": [
                        {"kind": "assigned_to", "assignees": ["me"]},
                        {"kind": "awaiting_response", "value": true}
                    ]
                }
            }),
        ),
    ];
    for (label, body) in cases {
        let response = crate::common::put_json_with_cookie(
            &router,
            "/api/organization/today-feeds/unanswered_inquiry",
            &cookie,
            body,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{label}"
        );
        let body = crate::common::body_json(response).await;
        assert_eq!(body["error"], "invalid_feed_rule", "{label}");
    }

    // Window present on the call feed is also a rule violation.
    let call_body = json!({
        "expected_revision": 1,
        "filter": {"version": 1, "clauses": [{"kind": "awaiting_call_outcome", "value": true}]},
        "fresh_within_hours": 24
    });
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/call_outcome_needed",
        &cookie,
        call_body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "invalid_feed_rule");
}

#[sqlx::test]
#[ignore]
async fn invalid_stage_reference_is_422_invalid_stage(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 invalid stage",
        "admin@d011-f4-invalid-stage.test",
        "member@d011-f4-invalid-stage.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-invalid-stage.test", PW).await;

    let body = json!({
        "expected_revision": 1,
        "filter": {
            "version": 1,
            "clauses": [
                {"kind": "assigned_to", "assignees": ["me"]},
                {"kind": "awaiting_response", "value": true},
                {"kind": "stage", "stage_ids": [Uuid::new_v4()]}
            ]
        },
        "fresh_within_hours": 24
    });
    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = crate::common::body_json(response).await;
    assert_eq!(body["error"], "invalid_stage");
}

#[sqlx::test]
#[ignore]
async fn a_body_over_128_kib_matches_the_existing_payload_too_large_mapping(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 oversize",
        "admin@d011-f4-oversize.test",
        "member@d011-f4-oversize.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie = crate::common::login_cookie(&router, "admin@d011-f4-oversize.test", PW).await;

    let response = oversize_put(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
    )
    .await;
    // Pin whichever status/envelope ApiError::PayloadTooLarge gives on
    // other routes (inbound_email.rs's oversize test: 413 with the JSON
    // envelope, not axum's default plain-text rejection).
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = crate::common::body_json(response).await;
    assert_eq!(body, json!({ "error": "payload_too_large" }));
}

#[sqlx::test]
#[ignore]
async fn preview_with_an_unknown_subject_is_404(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 preview unknown subject",
        "admin@d011-f4-preview-subject.test",
        "member@d011-f4-preview-subject.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie =
        crate::common::login_cookie(&router, "admin@d011-f4-preview-subject.test", PW).await;

    let body = json!({
        "filter": {
            "version": 1,
            "clauses": [
                {"kind": "assigned_to", "assignees": ["me"]},
                {"kind": "awaiting_response", "value": true}
            ]
        },
        "fresh_within_hours": 24,
        "subject_user_id": Uuid::new_v4(),
    });
    let response = crate::common::post_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry/preview",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
#[ignore]
async fn the_mutation_envelope_is_feed_and_changed(migrator_pool: PgPool) {
    let (_org_id, _admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 mutation envelope",
        "admin@d011-f4-mutation-envelope.test",
        "member@d011-f4-mutation-envelope.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie =
        crate::common::login_cookie(&router, "admin@d011-f4-mutation-envelope.test", PW).await;

    let response = crate::common::put_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry",
        &cookie,
        canonical_body(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::common::body_json(response).await;
    let keys: std::collections::BTreeSet<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(|s| s.as_str())
        .collect();
    assert_eq!(keys, ["feed", "changed"].into_iter().collect());
    assert_eq!(body["changed"], false); // canonical body collapses to no-op
}

#[sqlx::test]
#[ignore]
async fn the_preview_envelope_is_subject_items_truncated_description(migrator_pool: PgPool) {
    let (_org_id, admin_id, _member_id) = create_org_with_admin_and_member(
        &migrator_pool,
        "011d f4 preview envelope",
        "admin@d011-f4-preview-envelope.test",
        "member@d011-f4-preview-envelope.test",
    )
    .await;
    let router = crate::common::build_router(&migrator_pool).await;
    let cookie =
        crate::common::login_cookie(&router, "admin@d011-f4-preview-envelope.test", PW).await;

    let body = json!({
        "filter": {
            "version": 1,
            "clauses": [
                {"kind": "assigned_to", "assignees": ["me"]},
                {"kind": "awaiting_response", "value": true}
            ]
        },
        "fresh_within_hours": 24
    });
    let response = crate::common::post_json_with_cookie(
        &router,
        "/api/organization/today-feeds/unanswered_inquiry/preview",
        &cookie,
        body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::common::body_json(response).await;
    let keys: std::collections::BTreeSet<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(|s| s.as_str())
        .collect();
    assert_eq!(
        keys,
        ["subject", "items", "truncated", "description"]
            .into_iter()
            .collect()
    );
    assert_eq!(body["subject"]["id"], admin_id.to_string());
}

//! TRUST/CONTRACT: admitted metadata outer responses retain tenant-admin and
//! no-store boundaries, including errors before the handler/body extractor.
use crate::{common, db_activity_source, import_support};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::Response,
};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const ROOT: &str = "/api/migrations/fub/admitted-metadata-imports";

fn assert_response(response: &Response, expected: StatusCode, context: &str) {
    assert_eq!(response.status(), expected, "{context}");
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL),
        Some(&axum::http::HeaderValue::from_static("no-store")),
        "{context}"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_full_router_denials_are_source_free_and_not_cacheable(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let id = Uuid::new_v4();
    let plan = Uuid::new_v4();
    let record = Uuid::new_v4();
    let plan_path = format!("{ROOT}/{id}/plans/{plan}");
    let provenance = format!("/api/people/{id}/admitted-metadata-import-provenance");
    let mut reads = vec![
        (ROOT.to_owned(), StatusCode::OK),
        (format!("{ROOT}/{id}"), StatusCode::NOT_FOUND),
        (format!("{ROOT}/{id}/results"), StatusCode::NOT_FOUND),
        (format!("{ROOT}/{id}/remainder"), StatusCode::NOT_FOUND),
        (format!("{ROOT}/not-a-uuid"), StatusCode::BAD_REQUEST),
        (format!("{ROOT}?limit=0"), StatusCode::BAD_REQUEST),
        (format!("{ROOT}?limit=51"), StatusCode::BAD_REQUEST),
        (format!("{ROOT}?limit=bad"), StatusCode::BAD_REQUEST),
        (provenance.clone(), StatusCode::NOT_FOUND),
        (
            format!("{provenance}/{record}/fields/source.all"),
            StatusCode::NOT_FOUND,
        ),
        (
            format!("{ROOT}/{id}/results/{record}/fields/source.all"),
            StatusCode::NOT_FOUND,
        ),
    ];
    for leaf in ["mappings", "targets?kind=field", "records", "issues"] {
        reads.push((format!("{plan_path}/{leaf}"), StatusCode::NOT_FOUND));
    }
    for leaf in [
        format!("mappings/{record}/aliases"),
        format!("mappings/{record}/fields/all"),
        format!("records/{record}/fields/source.all"),
    ] {
        reads.push((format!("{plan_path}/{leaf}"), StatusCode::NOT_FOUND));
    }
    let mut writes = vec![ROOT.to_owned()];
    for action in ["plans", "confirm", "retry", "cancel", "remainder"] {
        writes.push(format!("{ROOT}/{id}/{action}"));
    }
    for mode in ["operational", "migration_review"] {
        if mode == "migration_review" {
            db_activity_source::completed_parent(&f).await;
        }
        let observed: String =
            sqlx::query_scalar("SELECT workspace_mode FROM organization WHERE id=$1")
                .bind(f.org)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        assert_eq!(observed, mode);
        let source_calls = f.reader.calls();
        for (cookie, denial) in [
            (f.cookie.as_str(), None),
            (f.member_cookie.as_str(), Some(StatusCode::FORBIDDEN)),
            ("", Some(StatusCode::UNAUTHORIZED)),
        ] {
            for (path, admin_status) in &reads {
                let response = common::get_with_cookie(&f.app, path, cookie).await;
                assert_response(
                    &response,
                    denial.unwrap_or(*admin_status),
                    &format!("{mode} GET {path}"),
                );
            }
            for path in &writes {
                let response = common::post_json_with_cookie(&f.app, path, cookie, json!({})).await;
                assert_response(
                    &response,
                    denial.unwrap_or(StatusCode::BAD_REQUEST),
                    &format!("{mode} POST {path} empty request"),
                );
                // A syntactically valid but oversized JSON string checks the
                // actual body limiter, while auth denial still takes precedence.
                let response = common::post_json_with_cookie(
                    &f.app,
                    path,
                    cookie,
                    json!("x".repeat(64 * 1024)),
                )
                .await;
                assert_response(
                    &response,
                    denial.unwrap_or(StatusCode::PAYLOAD_TOO_LARGE),
                    &format!("{mode} POST {path} oversized request"),
                );
            }
            for (path, request) in [
                (
                    ROOT.to_owned(),
                    json!({"request_id":id,"admission_id":id,"source_report_id":id,"organization_id":f.org}),
                ),
                (
                    format!("{ROOT}/{id}/plans"),
                    json!({"request_id":id,"expected_plan_revision":"1","mappings":[],"organization_id":f.org}),
                ),
                (
                    format!("{ROOT}/{id}/retry"),
                    json!({"request_id":id,"organization_id":f.org}),
                ),
            ] {
                let response = common::post_json_with_cookie(&f.app, &path, cookie, request).await;
                assert_response(
                    &response,
                    denial.unwrap_or(StatusCode::BAD_REQUEST),
                    &format!("{mode} POST {path} unknown authority field"),
                );
            }
            let response = f
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(ROOT)
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(header::COOKIE, cookie)
                        .body(Body::from("{"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_response(
                &response,
                denial.unwrap_or(StatusCode::BAD_REQUEST),
                &format!("{mode} malformed JSON"),
            );
        }
        assert_eq!(
            f.reader.calls(),
            source_calls,
            "metadata HTTP errors never call FUB"
        );
        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM migration_admitted_metadata_import),
                    (SELECT count(*) FROM migration_admitted_metadata_plan),
                    (SELECT count(*) FROM migration_admitted_metadata_receipt),
                    (SELECT count(*) FROM migration_metadata_catalog_readiness)",
        )
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(
            counts,
            (0, 0, 0, 0),
            "denials create no imports, plans, receipts or handover"
        );
    }
}

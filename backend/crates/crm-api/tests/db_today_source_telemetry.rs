//! Trace-sentinel coverage for Slice 011c Today source evaluation.
//!
//! This uses the production intake, saved-list command, source command, and
//! Today read paths. The capture starts only for the final read so its output
//! consists of the spans whose safe structured fields this slice introduces.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

use crate::common::{
    body_json, build_router, connect_as_app, create_org_with_stages_and_member, login_cookie,
    post_inquiry,
};
use crm_api::domain::envelope::{CommandContext, Origin};
use crm_api::domain::person::filter::{Clause, FilterDefinition, SourceClause};
use crm_api::domain::person::PersonVisibilityScope;
use crm_api::domain::saved_list::{self, CreateSavedList, SavedListScope};
use crm_api::domain::today::{self, EnableTodayWorkSource, TodayReason, TodaySourcesStatus};
use crm_api::ids::{CorrelationId, OrganizationId, UserId};

const PW: &str = "correct horse battery staple";
const LIST_NAME: &str = "SENTINEL_TODAY_LIST_NAME_DO_NOT_LOG";
const SOURCE_VALUE: &str = "sentinel_today_source_value_do_not_log";
const PERSON_FIRST_NAME: &str = "Mina";
const PERSON_LAST_NAME: &str = "Telemetry";
const PERSON_EMAIL: &str = "mina.telemetry@example.test";
const PERSON_PHONE: &str = "+1 555 555 0123";
const RAW_DATABASE_ERROR: &str = "permission denied for table today_work_source";

#[derive(Clone)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
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

fn command_context(organization_id: Uuid, actor_user_id: Uuid) -> CommandContext {
    CommandContext {
        organization_id: OrganizationId::new(organization_id),
        actor_user_id: UserId::new(actor_user_id),
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    }
}

fn has_field(line: &str, field: &str, value: &str) -> bool {
    line.contains(&format!("{field}={value}")) || line.contains(&format!("{field}=\"{value}\""))
}

fn assert_span_fields(captured: &str, span: &str, fields: &[(&str, &str)]) {
    assert!(
        captured.lines().any(|line| {
            line.contains(span)
                && fields
                    .iter()
                    .all(|(field, value)| has_field(line, field, value))
        }),
        "{span} omitted expected safe fields: {captured}"
    );
}

fn assert_span_records_duration(captured: &str, span: &str) {
    assert!(
        captured
            .lines()
            .any(|line| line.contains(span) && line.contains("duration_ms=")),
        "{span} omitted duration_ms: {captured}"
    );
}

/// A normal enabled source query emits only IDs, fixed filter-kind labels,
/// counts, booleans, and outcome labels. The live list definition and matched
/// Person deliberately carry sentinel private content that must stay absent.
#[sqlx::test]
#[ignore]
async fn today_source_spans_record_safe_structured_fields_without_private_content(
    migrator_pool: PgPool,
) {
    let (organization_id, actor_user_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Today source telemetry",
        "owner@today-source-telemetry.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@today-source-telemetry.test", PW).await;

    let intake = post_inquiry(
        &router,
        &cookie,
        SOURCE_VALUE,
        json!({
            "first_name": PERSON_FIRST_NAME,
            "last_name": PERSON_LAST_NAME,
            "email": PERSON_EMAIL,
            "phone": PERSON_PHONE,
        }),
        None,
    )
    .await;
    assert_eq!(intake.status(), StatusCode::CREATED);
    assert_eq!(body_json(intake).await["status"], "resolved");

    let list = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, actor_user_id),
        CreateSavedList {
            request_id: Uuid::new_v4(),
            scope: SavedListScope::Personal,
            name: LIST_NAME.to_string(),
            filter: FilterDefinition {
                version: 1,
                clauses: vec![Clause::Source(SourceClause {
                    sources: vec![SOURCE_VALUE.to_string()],
                })],
            },
        },
    )
    .await
    .unwrap();
    let enabled = today::enable_today_work_source(
        &app_pool,
        &command_context(organization_id, actor_user_id),
        EnableTodayWorkSource {
            list_id: list.list.id,
            expected_list_revision: list.list.revision,
        },
    )
    .await
    .unwrap();
    assert!(enabled.enabled);

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    let guard = tracing::subscriber::set_default(subscriber);
    let mut conn = app_pool.acquire().await.unwrap();
    let today = today::query(
        &mut conn,
        &PersonVisibilityScope::Organization(OrganizationId::new(organization_id)),
        UserId::new(actor_user_id),
        Utc::now(),
    )
    .await
    .unwrap();
    drop(conn);
    drop(guard);

    assert_eq!(today.sources.status, TodaySourcesStatus::Complete);
    assert!(today.sources.issues.is_empty());
    assert_eq!(today.items.len(), 1);
    assert!(today.items[0].reasons.iter().any(|reason| {
        matches!(reason, TodayReason::ListMember { list_id, .. } if *list_id == list.list.id)
    }));

    let normal_captured = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    // Kept distinct from the Uuid `organization_id`/`actor_user_id` bindings
    // above: the error-injection query below still needs the original Uuids.
    let organization_id_field = organization_id.to_string();
    let actor_user_id_field = actor_user_id.to_string();
    let list_id = list.list.id.to_string();
    assert_span_fields(
        &normal_captured,
        "today.query",
        &[
            ("organization_id", &organization_id_field),
            ("actor_id", &actor_user_id_field),
            ("outcome", "complete"),
            ("item_count", "1"),
            ("truncated", "false"),
            ("builtin_candidate_count", "1"),
            ("builtin_truncated", "false"),
            ("source_metadata_outcome", "complete"),
            ("enabled_source_count", "1"),
            ("successful_source_count", "1"),
            ("failed_source_count", "0"),
            ("list_candidate_count", "0"),
            ("list_item_count", "0"),
            ("list_truncated", "false"),
            ("sources_status", "complete"),
            ("source_issue_count", "0"),
        ],
    );
    assert_span_records_duration(&normal_captured, "today.query");
    assert_span_fields(
        &normal_captured,
        "today.source_enumeration",
        &[
            ("organization_id", &organization_id_field),
            ("actor_id", &actor_user_id_field),
            ("outcome", "complete"),
            ("source_count", "1"),
        ],
    );
    assert_span_records_duration(&normal_captured, "today.source_enumeration");
    assert_span_fields(
        &normal_captured,
        "today.source_evaluation",
        &[
            ("organization_id", &organization_id_field),
            ("actor_id", &actor_user_id_field),
            ("source_id", &list_id),
            ("filter_kinds", "source"),
            ("filter_clause_count", "1"),
            ("builtin_count", "1"),
            ("builtin_truncated", "false"),
            ("outcome", "complete"),
            ("membership_count", "1"),
            ("prefix_candidate_count", "0"),
        ],
    );
    assert_span_records_duration(&normal_captured, "today.source_evaluation");

    // A denied metadata read produces PostgreSQL's real permission error,
    // which the span contract must reduce to the safe `unavailable` label.
    sqlx::query("REVOKE SELECT ON TABLE today_work_source FROM crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
    let error_buffer = Arc::new(Mutex::new(Vec::new()));
    let error_subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(CaptureWriter(error_buffer.clone()))
            .with_ansi(false)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL),
    );
    let error_guard = tracing::subscriber::set_default(error_subscriber);
    let mut error_conn = app_pool.acquire().await.unwrap();
    let unavailable = today::query(
        &mut error_conn,
        &PersonVisibilityScope::Organization(OrganizationId::new(organization_id)),
        UserId::new(actor_user_id),
        Utc::now(),
    )
    .await
    .unwrap();
    drop(error_conn);
    drop(error_guard);
    sqlx::query("GRANT SELECT ON TABLE today_work_source TO crm_app")
        .execute(&migrator_pool)
        .await
        .unwrap();
    assert_eq!(unavailable.sources.status, TodaySourcesStatus::Unavailable);
    assert!(unavailable.sources.issues.is_empty());

    let error_captured = String::from_utf8(error_buffer.lock().unwrap().clone()).unwrap();
    assert_span_fields(
        &error_captured,
        "today.query",
        &[
            ("outcome", "sources_unavailable"),
            ("source_metadata_outcome", "unavailable"),
            ("sources_status", "unavailable"),
            ("source_issue_count", "0"),
        ],
    );
    // `source_count` stays unset here by design (mod.rs: an unavailable
    // metadata read must never report a *known* zero configured-source
    // count), so this outcome is the only field the span always carries.
    // Assert that absence explicitly, rather than only dropping the old
    // (incorrect) expectation that it equalled "0".
    assert_span_fields(
        &error_captured,
        "today.source_enumeration",
        &[("outcome", "unavailable")],
    );
    assert!(
        !error_captured
            .lines()
            .any(|line| line.contains("today.source_enumeration") && line.contains("source_count")),
        "today.source_enumeration must leave source_count unset (not zero) when metadata is unavailable: {error_captured}"
    );
    let captured = format!("{normal_captured}{error_captured}");

    for private_value in [
        LIST_NAME,
        SOURCE_VALUE,
        PERSON_FIRST_NAME,
        PERSON_LAST_NAME,
        PERSON_EMAIL,
        PERSON_PHONE,
        RAW_DATABASE_ERROR,
    ] {
        assert!(
            !captured.contains(private_value),
            "Today trace output leaked private value {private_value:?}: {captured}"
        );
    }
}

//! Required A8 negatives at typed-query and HTTP boundaries. Real retained
//! synthetic pages provide positive cursors; no source HTTP or fixture ciphertexts.
use std::sync::atomic::Ordering;

use axum::http::StatusCode;
use crm_api::{
    domain::migration::{
        commands, history_capture as h, history_capture_source::Stream, MigrationError,
    },
    ids::UserId,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    common::{body_json, get_with_cookie},
    db_history_capture_support as hs,
    import_support::Fixture,
};

const ROOT: &str = "/api/migrations/fub/history-captures";

fn page(cursor: Option<&str>) -> h::HistoryPage {
    h::HistoryPage {
        limit: Some(1),
        cursor: cursor.map(str::to_owned),
        family: Some("events".into()),
        disposition: Some("linked".into()),
        ..Default::default()
    }
}

async fn http(f: &Fixture, cookie: &str, uri: &str, expected: StatusCode) -> Value {
    let response = get_with_cookie(&f.app, uri, cookie).await;
    assert_eq!(response.status(), expected, "retained read status");
    let value = body_json(response).await;
    assert!(serde_json::to_vec(&value).unwrap().len() <= 512 * 1024);
    value
}

fn row_id(value: &Value) -> Uuid {
    Uuid::parse_str(value["records"][0]["id"].as_str().unwrap()).unwrap()
}

#[sqlx::test]
#[ignore]
async fn history_read_real_cursors_bind_run_endpoint_filters_and_page_size(migrator: PgPool) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    book.set_records(Stream::Events, (1..=3).map(hs::item).collect());
    book.set_records(Stream::Calls, (4..=5).map(hs::item).collect());
    let (run, _) = hs::propose(&f, parent).await;
    hs::confirm(&f, run).await;
    hs::drain(&f, &book).await;
    let (other_run, _) = hs::propose(&f, parent).await;
    let source_calls = book.count();
    let core_calls = f.reader.calls.load(Ordering::SeqCst);
    let first = h::records(&f.pool, &f.key, &f.ctx, run, page(None))
        .await
        .unwrap();
    let record = row_id(&first);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = h::records(&f.pool, &f.key, &f.ctx, run, page(Some(&cursor)))
        .await
        .unwrap();
    assert_ne!(row_id(&second), record);
    assert_eq!(second["capture_sequence"], first["capture_sequence"]);
    let last = h::records(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        page(second["next_cursor"].as_str()),
    )
    .await
    .unwrap();
    assert!(last["next_cursor"].is_null());
    assert_eq!(
        [record, row_id(&second), row_id(&last)]
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    let prefix = format!("{ROOT}/{run}/records");
    let continued = http(
        &f,
        &f.cookie,
        &format!("{prefix}?limit=1&family=events&disposition=linked&cursor={cursor}"),
        StatusCode::OK,
    )
    .await;
    assert_eq!(continued["records"], second["records"]);
    assert_eq!(continued["capture_sequence"], second["capture_sequence"]);

    let mut tampered = cursor.as_bytes().to_vec();
    tampered[0] = if tampered[0] == b'A' { b'B' } else { b'A' };
    for (label, invalid) in [
        ("malformed base64", "not.base64!".into()),
        ("too short", "A".into()),
        ("too long", "A".repeat(4097)),
        (
            "authenticated cursor tampered",
            String::from_utf8(tampered).unwrap(),
        ),
    ] {
        assert!(
            matches!(
                h::records(&f.pool, &f.key, &f.ctx, run, page(Some(&invalid))).await,
                Err(MigrationError::InvalidInput)
            ),
            "{label}"
        );
        http(
            &f,
            &f.cookie,
            &format!("{prefix}?limit=1&family=events&disposition=linked&cursor={invalid}"),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    for (label, query) in [
        (
            "family",
            "limit=1&family=calls&disposition=linked".to_owned(),
        ),
        (
            "disposition",
            "limit=1&family=events&disposition=parent_excluded".to_owned(),
        ),
        (
            "record",
            format!("limit=1&family=events&disposition=linked&record_id={record}"),
        ),
        (
            "page size",
            "limit=2&family=events&disposition=linked".to_owned(),
        ),
    ] {
        let mut changed = page(Some(&cursor));
        match label {
            "family" => changed.family = Some("calls".into()),
            "disposition" => changed.disposition = Some("parent_excluded".into()),
            "record" => changed.record_id = Some(record),
            "page size" => changed.limit = Some(2),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                h::records(&f.pool, &f.key, &f.ctx, run, changed).await,
                Err(MigrationError::InvalidInput)
            ),
            "changed {label}"
        );
        http(
            &f,
            &f.cookie,
            &format!("{prefix}?{query}&cursor={cursor}"),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    assert!(matches!(
        h::records(&f.pool, &f.key, &f.ctx, other_run, page(Some(&cursor))).await,
        Err(MigrationError::InvalidInput)
    ));
    http(
        &f,
        &f.cookie,
        &format!(
            "{ROOT}/{other_run}/records?limit=1&family=events&disposition=linked&cursor={cursor}"
        ),
        StatusCode::BAD_REQUEST,
    )
    .await;

    let list_page = h::list(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::HistoryPage {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let list_cursor = list_page["next_cursor"].as_str().unwrap().to_owned();
    let list_next = h::list(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        h::HistoryPage {
            limit: Some(1),
            cursor: Some(list_cursor.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(list_next["next_cursor"].is_null());
    assert_ne!(
        list_page["captures"][0]["id"],
        list_next["captures"][0]["id"]
    );
    let http_next = http(
        &f,
        &f.cookie,
        &format!("{ROOT}?limit=1&cursor={list_cursor}"),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        http_next["captures"][0]["id"],
        list_next["captures"][0]["id"]
    );
    // Both directions use real valid cursor envelopes, not malformed substitutes.
    assert!(matches!(
        h::records(&f.pool, &f.key, &f.ctx, run, page(Some(&list_cursor))).await,
        Err(MigrationError::InvalidInput)
    ));
    http(
        &f,
        &f.cookie,
        &format!("{prefix}?limit=1&family=events&disposition=linked&cursor={list_cursor}"),
        StatusCode::BAD_REQUEST,
    )
    .await;
    for (label, list_query, suffix) in [
        (
            "records endpoint",
            h::HistoryPage {
                limit: Some(1),
                cursor: Some(cursor.clone()),
                ..Default::default()
            },
            format!("limit=1&cursor={cursor}"),
        ),
        (
            "parent filter",
            h::HistoryPage {
                limit: Some(1),
                cursor: Some(list_cursor.clone()),
                parent_import_id: Some(parent),
                ..Default::default()
            },
            format!("limit=1&parent_import_id={parent}&cursor={list_cursor}"),
        ),
        (
            "list page size",
            h::HistoryPage {
                limit: Some(2),
                cursor: Some(list_cursor.clone()),
                ..Default::default()
            },
            format!("limit=2&cursor={list_cursor}"),
        ),
    ] {
        assert!(
            matches!(
                h::list(&f.pool, &f.key, &f.policy, &f.ctx, list_query).await,
                Err(MigrationError::InvalidInput)
            ),
            "{label}"
        );
        http(
            &f,
            &f.cookie,
            &format!("{ROOT}?{suffix}"),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    assert_eq!(
        book.count(),
        source_calls,
        "all retained queries are source-free"
    );
    assert_eq!(f.reader.calls.load(Ordering::SeqCst), core_calls);
}

#[sqlx::test]
#[ignore]
async fn history_read_invalid_filters_current_authority_and_foreign_evidence_fail_closed(
    migrator: PgPool,
) {
    let (f, parent, book) = hs::fixture(&migrator).await;
    book.set_records(Stream::Events, (1..=3).map(hs::item).collect());
    let (run, _) = hs::propose(&f, parent).await;
    hs::confirm(&f, run).await;
    hs::drain(&f, &book).await;
    let (foreign, _, foreign_book) = hs::fixture(&migrator).await;
    let calls = (
        book.count(),
        f.reader.calls.load(Ordering::SeqCst),
        foreign.reader.calls.load(Ordering::SeqCst),
    );
    let first = h::records(&f.pool, &f.key, &f.ctx, run, page(None))
        .await
        .unwrap();
    let record = row_id(&first);
    let cursor = first["next_cursor"].as_str().unwrap();
    let prefix = format!("{ROOT}/{run}/records");
    let detail = h::record_detail(&f.pool, &f.key, &f.ctx, run, record)
        .await
        .unwrap();
    assert_eq!(detail["integrity_status"], "verified_projection");
    assert!(matches!(
        h::records(
            &foreign.pool,
            &foreign.key,
            &foreign.ctx,
            run,
            page(Some(cursor))
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    http(
        &f,
        &foreign.cookie,
        &format!("{prefix}?limit=1&family=events&disposition=linked&cursor={cursor}"),
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert!(matches!(
        h::record_detail(&foreign.pool, &foreign.key, &foreign.ctx, run, record).await,
        Err(MigrationError::NotFound)
    ));
    http(
        &f,
        &foreign.cookie,
        &format!("{prefix}/{record}"),
        StatusCode::NOT_FOUND,
    )
    .await;
    let mut member = f.ctx.clone();
    member.actor_user_id = UserId::new(f.member);
    assert!(matches!(
        h::record_detail(&f.pool, &f.key, &member, run, record).await,
        Err(MigrationError::Forbidden)
    ));
    http(
        &f,
        &f.member_cookie,
        &format!("{prefix}/{record}"),
        StatusCode::FORBIDDEN,
    )
    .await;
    // Previously accepted server context must re-read current membership.
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(matches!(
        h::record_detail(&f.pool, &f.key, &f.ctx, run, record).await,
        Err(MigrationError::Forbidden)
    ));
    http(
        &f,
        &f.cookie,
        &format!("{prefix}/{record}"),
        StatusCode::FORBIDDEN,
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&migrator)
    .await
    .unwrap();
    for (label, query, suffix) in [
        (
            "zero limit",
            h::HistoryPage {
                limit: Some(0),
                ..Default::default()
            },
            "limit=0".into(),
        ),
        (
            "large limit",
            h::HistoryPage {
                limit: Some(51),
                ..Default::default()
            },
            "limit=51".into(),
        ),
        (
            "unknown family",
            h::HistoryPage {
                family: Some("email".into()),
                ..Default::default()
            },
            "family=email".into(),
        ),
        (
            "unknown disposition",
            h::HistoryPage {
                disposition: Some("all".into()),
                ..Default::default()
            },
            "disposition=all".into(),
        ),
        (
            "list-only filter",
            h::HistoryPage {
                parent_import_id: Some(parent),
                ..Default::default()
            },
            format!("parent_import_id={parent}"),
        ),
    ] {
        assert!(
            matches!(
                h::records(&f.pool, &f.key, &f.ctx, run, query).await,
                Err(MigrationError::InvalidInput)
            ),
            "{label}"
        );
        http(
            &f,
            &f.cookie,
            &format!("{prefix}?{suffix}"),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    for (label, query, suffix) in [
        (
            "zero limit",
            h::HistoryPage {
                limit: Some(0),
                ..Default::default()
            },
            "limit=0".into(),
        ),
        (
            "large limit",
            h::HistoryPage {
                limit: Some(51),
                ..Default::default()
            },
            "limit=51".into(),
        ),
        (
            "record family",
            h::HistoryPage {
                family: Some("events".into()),
                ..Default::default()
            },
            "family=events".into(),
        ),
        (
            "record disposition",
            h::HistoryPage {
                disposition: Some("linked".into()),
                ..Default::default()
            },
            "disposition=linked".into(),
        ),
        (
            "record identity",
            h::HistoryPage {
                record_id: Some(record),
                ..Default::default()
            },
            format!("record_id={record}"),
        ),
    ] {
        assert!(
            matches!(
                h::list(&f.pool, &f.key, &f.policy, &f.ctx, query).await,
                Err(MigrationError::InvalidInput)
            ),
            "{label}"
        );
        http(
            &f,
            &f.cookie,
            &format!("{ROOT}?{suffix}"),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    for suffix in [
        "limit=-1",
        "limit=65536",
        "record_id=bad-uuid",
        "parent_import_id=bad-uuid",
    ] {
        http(
            &f,
            &f.cookie,
            &format!("{prefix}?{suffix}"),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    let connection: Uuid = sqlx::query_scalar("SELECT connection_id FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2")
        .bind(run).bind(f.org).fetch_one(&f.pool).await.unwrap();
    commands::disconnect_fub(
        &f.pool,
        &f.ctx,
        commands::DisconnectFub {
            connection_id: connection,
        },
    )
    .await
    .unwrap();
    let after = h::record_detail(&f.pool, &f.key, &f.ctx, run, record)
        .await
        .unwrap();
    assert_eq!(after, detail, "retained detail survives source disconnect");
    assert_eq!(
        http(&f, &f.cookie, &format!("{prefix}/{record}"), StatusCode::OK).await,
        detail
    );
    let continued = h::records(&f.pool, &f.key, &f.ctx, run, page(Some(cursor)))
        .await
        .unwrap();
    assert_eq!(continued["records"].as_array().unwrap().len(), 1);
    assert_eq!(
        (
            book.count(),
            f.reader.calls.load(Ordering::SeqCst),
            foreign.reader.calls.load(Ordering::SeqCst)
        ),
        calls
    );
    assert_eq!(foreign_book.count(), 0);
}

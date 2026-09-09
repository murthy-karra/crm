//! DB-backed tests for saved lists' interplay with tags (Slice 011e e2
//! step 3, docs/specs/SLICE_011e.md §9.14): a foreign or deleted tag id
//! at write time is 422 `invalid_tag`, an already-stored tag clause that
//! goes invalid is reported (and repaired) on detail and count reads, and
//! the description resolves tag names. Split from a single
//! db_saved_lists.rs (item 3 of the LATER batch,
//! docs/tasks/LATER_BATCH_2026-09-08.md, no test bodies changed): the
//! core saved-list CRUD/permission/HTTP tests are in db_saved_lists.rs
//! and the Slice 011b-sort feature is in db_saved_lists_sort.rs. This
//! file's fixtures (`insert_tag`, `hard_delete_tag`) are used only here,
//! so they stayed local rather than moving to tests/common/. Run only
//! via ./scripts/check-db.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::common::saved_lists::PW;
use crate::common::{
    body_json, build_router, create_org_with_stages_and_member, get_with_cookie, login_cookie,
    post_json_with_cookie, put_json_with_cookie,
};

// --- Slice 011e e2 step 3 (docs/specs/SLICE_011e.md §9.14) ---------------

async fn insert_tag(pool: &PgPool, organization_id: Uuid, created_by: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tag (organization_id, name, created_by_user_id) VALUES ($1, $2, $3) \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(name)
    .bind(created_by)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn hard_delete_tag(pool: &PgPool, tag_id: Uuid) {
    sqlx::query("DELETE FROM tag WHERE id = $1")
        .bind(tag_id)
        .execute(pool)
        .await
        .unwrap();
}

/// §9.14 write time: `POST`/`PUT /api/saved-lists` with a foreign or
/// deleted tag id are byte-identical 422 `invalid_tag` bodies, exactly the
/// existing non-leaking posture for `invalid_stage`/`invalid_assignee`.
#[sqlx::test]
#[ignore]
async fn saved_list_write_time_foreign_or_deleted_tag_id_is_422_invalid_tag(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list tag write",
        "owner@saved-list-tag-write.test",
        "Owner",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-list-tag-write.test", PW).await;

    // A tag id that never existed.
    let random_uuid = Uuid::new_v4();
    let random_resp = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "Random tag",
            "filter": {"version": 1, "clauses": [{"kind": "tags", "tag_ids": [random_uuid]}]},
        }),
    )
    .await;
    assert_eq!(random_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let random_json = body_json(random_resp).await;
    assert_eq!(random_json, json!({"error": "invalid_tag"}));

    // A tag that existed and was deleted.
    let tag_id = insert_tag(&migrator_pool, organization_id, owner_id, "Deleted").await;
    hard_delete_tag(&migrator_pool, tag_id).await;
    let deleted_resp = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "Deleted tag",
            "filter": {"version": 1, "clauses": [{"kind": "tags", "tag_ids": [tag_id]}]},
        }),
    )
    .await;
    assert_eq!(deleted_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let deleted_json = body_json(deleted_resp).await;
    assert_eq!(
        deleted_json, random_json,
        "a foreign uuid and a deleted tag id must be byte-identical 422s"
    );

    // PUT (update): a previously valid list updated to name a foreign tag.
    let created = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "Updatable",
            "filter": {"version": 1, "clauses": []},
        }),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_json = body_json(created).await;
    let list_id = created_json["list"]["id"].as_str().unwrap().to_string();

    let update_resp = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{list_id}"),
        &cookie,
        json!({
            "expected_revision": 1,
            "name": "Updatable",
            "filter": {"version": 1, "clauses": [{"kind": "not_tags", "tag_ids": [tag_id]}]},
        }),
    )
    .await;
    assert_eq!(update_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(update_resp).await,
        json!({"error": "invalid_tag"})
    );
}

/// §9.14 read time: a saved list naming a tag that is deleted AFTER
/// creation reports `filter_error: "invalid_tag"` on the detail read with
/// the rest of the metadata (name, revision) preserved, `count` is 422
/// `invalid_tag`, and the writer repairs the list by saving a filter
/// without the clause. `unsupported_filter` must never appear.
#[sqlx::test]
#[ignore]
async fn saved_list_detail_and_count_report_invalid_tag_and_writer_repairs(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list tag read",
        "owner@saved-list-tag-read.test",
        "Owner",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-list-tag-read.test", PW).await;

    let tag_id = insert_tag(&migrator_pool, organization_id, owner_id, "Will be deleted").await;

    let created = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "Tag reader",
            "filter": {"version": 1, "clauses": [{"kind": "tags", "tag_ids": [tag_id]}]},
        }),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_json = body_json(created).await;
    let list_id = created_json["list"]["id"].as_str().unwrap().to_string();
    assert_eq!(created_json["list"]["revision"], 1);

    // Deleted after the list already names it.
    hard_delete_tag(&migrator_pool, tag_id).await;

    let detail =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{list_id}"), &cookie).await)
            .await;
    assert_eq!(detail["filter_error"], "invalid_tag");
    assert_ne!(detail["filter_error"], "unsupported_filter");
    assert_eq!(detail["list"]["name"], "Tag reader");
    assert_eq!(detail["list"]["revision"], 1);

    let count_resp = get_with_cookie(
        &router,
        &format!("/api/saved-lists/{list_id}/count?revision=1"),
        &cookie,
    )
    .await;
    assert_eq!(count_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let count_json = body_json(count_resp).await;
    assert_eq!(count_json, json!({"error": "invalid_tag"}));

    // The writer repairs by saving a definition without the clause.
    let repaired = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{list_id}"),
        &cookie,
        json!({
            "expected_revision": 1,
            "name": "Tag reader",
            "filter": {"version": 1, "clauses": []},
        }),
    )
    .await;
    assert_eq!(repaired.status(), StatusCode::OK);
    let repaired_json = body_json(repaired).await;
    assert!(repaired_json["changed"].as_bool().unwrap());

    let repaired_detail =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{list_id}"), &cookie).await)
            .await;
    assert_eq!(repaired_detail["filter_error"], Value::Null);
}

/// docs/specs/SLICE_011e.md §9.16: `describe()`'s exact strings for one and
/// several tag names, and the "an unknown tag" placeholder for an id with
/// no resolvable name -- proving the saved-list detail's `FilterNames`
/// loader (`saved_list::queries::filter_names`) actually resolves tag
/// names from the database, not just that `describe()` itself is correct
/// (already unit-pinned in `crm-app`).
#[sqlx::test]
#[ignore]
async fn saved_list_detail_description_resolves_tag_names(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved list tag names",
        "owner@saved-list-tag-names.test",
        "Owner",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-list-tag-names.test", PW).await;

    let investor = insert_tag(&migrator_pool, organization_id, owner_id, "Investor").await;
    let past_client = insert_tag(&migrator_pool, organization_id, owner_id, "Past client").await;

    // Several names: joined "or", in clause order.
    let both = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "Tag names",
            "filter": {
                "version": 1,
                "clauses": [{"kind": "tags", "tag_ids": [investor, past_client]}]
            },
        }),
    )
    .await;
    assert_eq!(both.status(), StatusCode::CREATED);
    let both_id = body_json(both).await["list"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let both_detail =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{both_id}"), &cookie).await)
            .await;
    assert_eq!(
        both_detail["description"],
        json!(["Tagged Investor or Past client"])
    );

    // One name, `not_tags`.
    let one = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "One tag name",
            "filter": {
                "version": 1,
                "clauses": [{"kind": "not_tags", "tag_ids": [investor]}]
            },
        }),
    )
    .await;
    assert_eq!(one.status(), StatusCode::CREATED);
    let one_id = body_json(one).await["list"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let one_detail =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{one_id}"), &cookie).await)
            .await;
    assert_eq!(one_detail["description"], json!(["Not tagged Investor"]));

    // Unknown placeholder: the id exists at write time (passes reference
    // validation) but is deleted before this detail read, so `describe()`
    // sees an id absent from the resolved name map.
    hard_delete_tag(&migrator_pool, investor).await;
    let after_delete_detail =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{one_id}"), &cookie).await)
            .await;
    assert_eq!(
        after_delete_detail["description"],
        json!(["Not tagged an unknown tag"])
    );
    assert_eq!(after_delete_detail["filter_error"], "invalid_tag");
}

//! DB-backed tests for the Slice 011b-sort feature: sort persists with
//! the saved-list definition, revision bumps only when sort changes,
//! retry-identity replay with and without an explicit sort, the HTTP
//! sort contract (round trip, malformed shapes, 404 precedence), member
//! vs. owner sort-write permission, and stored-sort repair. Split from a
//! single db_saved_lists.rs (item 3 of the LATER batch,
//! docs/tasks/LATER_BATCH_2026-09-08.md, no test bodies changed): the
//! core saved-list CRUD/permission/HTTP tests stayed in
//! db_saved_lists.rs, and the tag interplay (Slice 011e e2) moved to
//! db_saved_lists_tags.rs. The shared fixture helpers (`create_list`,
//! `create_list_with_sort`, `empty_filter`, `command_context`,
//! `auth_context`, `assert_status_body`) moved to
//! tests/common/saved_lists.rs, used by this file and db_saved_lists.rs.
//! Run only via ./scripts/check-db.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::common::{
    add_membership_with, body_json, build_router, connect_as_app,
    create_org_with_stages_and_member, create_user, get_with_cookie, login_cookie,
    post_json_with_cookie, put_json_with_cookie,
};
use crm_api::domain::admin::{MembershipStatus, Role};
use crm_api::domain::person::sort::PersonSort;
use crm_api::domain::saved_list::{
    self, CreateSavedList, DeleteSavedList, SavedListError, SavedListScope, UpdateSavedList,
};

use crate::common::saved_lists::*;

// --- Slice 011b-sort: sort is part of the definition ----------------------

/// §11.9: sort persists with the definition, a sort-only change bumps
/// revision, re-saving the identical sort is a no-op, and saving the wire
/// form of the default (`created.desc`) normalizes to a stored `NULL` pair.
#[sqlx::test]
#[ignore]
async fn saved_list_sort_is_part_of_the_definition_and_bumps_revision_only_when_it_changes(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort lifecycle",
        "owner@saved-sort-lifecycle.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;

    let name_asc = PersonSort::parse("name.asc").unwrap();
    let created = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Sorted list",
        empty_filter(),
        Some(name_asc),
    )
    .await;
    assert!(created.created);
    assert_eq!(created.list.revision, 1);

    let auth = auth_context(organization_id, owner_id, Role::Member);
    {
        let mut conn = app_pool.acquire().await.unwrap();
        let detail = saved_list::saved_list_detail(&mut conn, &auth, created.list.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail.sort, Some(name_asc));
    }

    // Re-saving the exact same name/filter/sort is a no-op: revision does
    // not move.
    let no_op = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 1,
            name: "Sorted list".to_string(),
            filter: empty_filter(),
            sort: Some(name_asc),
        },
    )
    .await
    .unwrap();
    assert!(!no_op.changed, "identical name/filter/sort must no-op");
    assert_eq!(no_op.list.revision, 1);

    // A sort-only change (same name, same filter) still bumps revision.
    let stage_desc = PersonSort::parse("stage.desc").unwrap();
    let sort_only = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 1,
            name: "Sorted list".to_string(),
            filter: empty_filter(),
            sort: Some(stage_desc),
        },
    )
    .await
    .unwrap();
    assert!(sort_only.changed, "a sort-only change must bump revision");
    assert_eq!(sort_only.list.revision, 2);
    {
        let mut conn = app_pool.acquire().await.unwrap();
        let detail_after = saved_list::saved_list_detail(&mut conn, &auth, created.list.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(detail_after.sort, Some(stage_desc));
    }

    // Saving `created.desc` (the wire form of the default) normalizes to a
    // stored NULL pair, so detail returns `sort: None`.
    let back_to_default = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 2,
            name: "Sorted list".to_string(),
            filter: empty_filter(),
            sort: Some(PersonSort::DEFAULT),
        },
    )
    .await
    .unwrap();
    assert!(
        back_to_default.changed,
        "created.desc differs from the current stage.desc, so this bumps"
    );
    assert_eq!(back_to_default.list.revision, 3);
    {
        let mut conn = app_pool.acquire().await.unwrap();
        let detail_default = saved_list::saved_list_detail(&mut conn, &auth, created.list.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            detail_default.sort, None,
            "created.desc normalizes to a NULL stored pair"
        );
    }
    let stored: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT sort_key, sort_direction FROM saved_list WHERE id = $1")
            .bind(created.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(stored, (None, None));

    // Re-saving `created.desc` again (already the stored default) is a
    // no-op.
    let default_no_op = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 3,
            name: "Sorted list".to_string(),
            filter: empty_filter(),
            sort: None,
        },
    )
    .await
    .unwrap();
    assert!(
        !default_no_op.changed,
        "an absent sort must equal an already-default stored sort"
    );
    assert_eq!(default_no_op.list.revision, 3);
}

/// §11.9: the same retry token replays identically whether `sort` is
/// absent or the literal `"created.desc"` (both normalize to the same
/// fingerprint), while a genuinely different sort on the same token is a
/// real conflict.
#[sqlx::test]
#[ignore]
async fn saved_list_create_retry_replays_identically_whether_sort_is_absent_or_created_desc(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort retry",
        "owner@saved-sort-retry.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let request_id = Uuid::new_v4();

    let created = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "Retry sort",
        empty_filter(),
        None,
    )
    .await;
    assert!(created.created);

    let replay = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "Retry sort",
        empty_filter(),
        Some(PersonSort::DEFAULT),
    )
    .await;
    assert!(
        !replay.created,
        "created.desc normalizes identically to an absent sort"
    );
    assert_eq!(replay.list.id, created.list.id);

    // A genuinely different sort on the same retry token is a real
    // conflict, not a replay.
    let conflicting = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        CreateSavedList {
            request_id,
            scope: SavedListScope::Personal,
            name: "Retry sort".to_string(),
            filter: empty_filter(),
            sort: Some(PersonSort::parse("name.asc").unwrap()),
        },
    )
    .await;
    assert!(matches!(conflicting, Err(SavedListError::RequestConflict)));
}

/// §11.9: a member's sort-only PUT on a shared list is still 403 —
/// authorization is unchanged by the addition of `sort`.
#[sqlx::test]
#[ignore]
async fn saved_list_member_sort_only_update_on_shared_list_is_forbidden(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort shared perms",
        "admin@saved-sort-perms.test",
        "Admin",
        PW,
    )
    .await;
    // `create_org_with_stages_and_member` already added this user as a
    // `member`; promote in place rather than re-inserting (which would
    // collide with the existing `organization_membership` primary key).
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let member_id = create_user(&migrator_pool, "member@saved-sort-perms.test", "Member", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Shared sortable",
        empty_filter(),
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let member_cookie = login_cookie(&router, "member@saved-sort-perms.test", PW).await;
    let resp = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{}", shared.list.id),
        &member_cookie,
        json!({
            "expected_revision": 1,
            "name": "Shared sortable",
            "filter": {"version": 1, "clauses": []},
            "sort": "name.asc",
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(resp).await["error"], "forbidden");
}

/// §11.9: delete clears both sort columns along with name/filter.
#[sqlx::test]
#[ignore]
async fn saved_list_delete_clears_sort_columns(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort delete",
        "owner@saved-sort-delete.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let created = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Delete sort",
        empty_filter(),
        Some(PersonSort::parse("assignee.desc").unwrap()),
    )
    .await;

    let deleted = saved_list::delete_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        DeleteSavedList {
            list_id: created.list.id,
            expected_revision: 1,
        },
    )
    .await
    .unwrap();
    assert!(deleted.deleted);

    let stored: (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT name, filter::text, sort_key, sort_direction FROM saved_list WHERE id = $1",
    )
    .bind(created.list.id.as_uuid())
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(stored, (None, None, None, None));
}

/// §5, §11.9: a stored `(sort_key, sort_direction)` pair this binary
/// cannot read fails the WHOLE definition closed — `filter: null, sort:
/// null, filter_error: "unsupported_filter"` on detail, 422
/// `unsupported_filter` on count — even though the stored filter is a
/// perfectly valid empty filter. The CHECK constraints make this
/// unreachable through any normal write (the guard's whole point), so this
/// simulates the only way it could occur (migration/binary skew, per the
/// migration's own comment) by momentarily relaxing the CHECK via the
/// migrator connection, exactly as the deep-JSONB fail-closed tests above
/// simulate skewed filter content.
#[sqlx::test]
#[ignore]
async fn unrecognized_stored_sort_pair_fails_closed_on_detail_and_count(migrator_pool: PgPool) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort skew",
        "owner@saved-sort-skew.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-sort-skew.test", PW).await;

    let created = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Skewed sort",
        empty_filter(),
    )
    .await;

    sqlx::query("ALTER TABLE saved_list DROP CONSTRAINT saved_list_sort_key_check")
        .execute(&migrator_pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE saved_list SET sort_key = 'distance', sort_direction = 'asc' WHERE id = $1",
    )
    .bind(created.list.id.as_uuid())
    .execute(&migrator_pool)
    .await
    .unwrap();

    let detail_body = body_json(
        get_with_cookie(
            &router,
            &format!("/api/saved-lists/{}", created.list.id),
            &cookie,
        )
        .await,
    )
    .await;
    assert_eq!(
        detail_body["filter"],
        Value::Null,
        "an unreadable sort forces filter: null too, per the single fail-closed disposition"
    );
    assert_eq!(detail_body["sort"], Value::Null);
    assert_eq!(detail_body["filter_error"], "unsupported_filter");

    let count_resp = get_with_cookie(
        &router,
        &format!("/api/saved-lists/{}/count?revision=1", created.list.id),
        &cookie,
    )
    .await;
    assert_eq!(count_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(count_resp).await["error"], "unsupported_filter");

    // Delete must still work on an unreadable-sort row.
    let delete_resp = delete_json(
        &router,
        &format!("/api/saved-lists/{}", created.list.id),
        &cookie,
        json!({"expected_revision": 1}),
    )
    .await;
    assert_eq!(delete_resp.status(), StatusCode::OK);
}

// --- Independent review follow-ups: HTTP-level `sort` coverage ------------

/// §6, §11.9: the HTTP surface for `sort` on saved-list create/update/detail
/// — a valid token round-trips, a malformed token is 400 `malformed_request`
/// on create, an update's 400 for a bad sort precedes the 404 a nonexistent
/// id would otherwise produce, and clearing a stored sort with an explicit
/// `null` both nulls the stored pair and bumps the revision exactly once.
#[sqlx::test]
#[ignore]
async fn saved_list_http_sort_round_trips_rejects_malformed_and_precedes_404(
    migrator_pool: PgPool,
) {
    create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort HTTP",
        "owner@saved-sort-http.test",
        "Owner",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-sort-http.test", PW).await;

    // (a) POST with a valid sort -> 201, and GET detail echoes it.
    let request_id = Uuid::new_v4();
    let create_resp = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": request_id,
            "scope": "personal",
            "name": "HTTP sort",
            "filter": {"version": 1, "clauses": []},
            "sort": "name.asc",
        }),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = body_json(create_resp).await;
    let id = created["list"]["id"].as_str().unwrap().to_string();

    let detail =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{id}"), &cookie).await).await;
    assert_eq!(detail["sort"], "name.asc");

    // (b) A malformed token in the body is 400 `malformed_request`.
    let bad_create = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "bad sort",
            "filter": {"version": 1, "clauses": []},
            "sort": "NAME.asc",
        }),
    )
    .await;
    assert_eq!(bad_create.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(bad_create).await,
        json!({"error": "malformed_request"})
    );

    // (c) PUT to a random, nonexistent id with an invalid sort is 400, not
    // 404 — the body decode failure happens before any resource lookup.
    let random_id = Uuid::new_v4();
    let bad_update = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{random_id}"),
        &cookie,
        json!({
            "expected_revision": 1,
            "name": "does not exist",
            "filter": {"version": 1, "clauses": []},
            "sort": "NAME.asc",
        }),
    )
    .await;
    assert_eq!(bad_update.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(bad_update).await,
        json!({"error": "malformed_request"})
    );

    // (d) PUT with `"sort": null` on a list that had a sort clears the
    // stored pair to NULL and bumps the revision exactly once.
    let clear_resp = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{id}"),
        &cookie,
        json!({
            "expected_revision": 1,
            "name": "HTTP sort",
            "filter": {"version": 1, "clauses": []},
            "sort": Value::Null,
        }),
    )
    .await;
    assert_eq!(clear_resp.status(), StatusCode::OK);
    let cleared = body_json(clear_resp).await;
    assert_eq!(cleared["changed"], true);
    assert_eq!(cleared["list"]["revision"], 2);

    let detail_after =
        body_json(get_with_cookie(&router, &format!("/api/saved-lists/{id}"), &cookie).await).await;
    assert_eq!(detail_after["sort"], Value::Null);
    assert_eq!(detail_after["list"]["revision"], 2);
}

/// §6: a `sort` value that decodes to valid JSON but the wrong shape (a
/// number, an object) is still 400 `malformed_request`, not a 5xx or a
/// silently-ignored field, on both create and update.
#[sqlx::test]
#[ignore]
async fn saved_list_http_sort_rejects_non_string_json_shapes_on_create_and_update(
    migrator_pool: PgPool,
) {
    create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort shapes",
        "owner@saved-sort-shapes.test",
        "Owner",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-sort-shapes.test", PW).await;

    for bad_sort in [json!(42), json!({"key": "name"})] {
        let create_resp = post_json_with_cookie(
            &router,
            "/api/saved-lists",
            &cookie,
            json!({
                "request_id": Uuid::new_v4(),
                "scope": "personal",
                "name": "bad sort shape",
                "filter": {"version": 1, "clauses": []},
                "sort": bad_sort,
            }),
        )
        .await;
        assert_eq!(create_resp.status(), StatusCode::BAD_REQUEST, "{bad_sort}");
        assert_eq!(
            body_json(create_resp).await,
            json!({"error": "malformed_request"}),
            "{bad_sort}"
        );
    }

    // A real list to PUT against.
    let create_resp = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": Uuid::new_v4(),
            "scope": "personal",
            "name": "target",
            "filter": {"version": 1, "clauses": []},
        }),
    )
    .await;
    let id = body_json(create_resp).await["list"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for bad_sort in [json!(42), json!({"key": "name"})] {
        let update_resp = put_json_with_cookie(
            &router,
            &format!("/api/saved-lists/{id}"),
            &cookie,
            json!({
                "expected_revision": 1,
                "name": "target",
                "filter": {"version": 1, "clauses": []},
                "sort": bad_sort,
            }),
        )
        .await;
        assert_eq!(update_resp.status(), StatusCode::BAD_REQUEST, "{bad_sort}");
        assert_eq!(
            body_json(update_resp).await,
            json!({"error": "malformed_request"}),
            "{bad_sort}"
        );
    }
}

/// §6, §7: a malformed sort in a PUT body fails decode before the command
/// layer ever checks shared-list edit authorization, so a member (who would
/// otherwise get 403 on a well-formed edit attempt) gets 400 instead.
#[sqlx::test]
#[ignore]
async fn saved_list_member_put_with_bad_sort_on_shared_list_is_400_not_403(migrator_pool: PgPool) {
    let (organization_id, admin_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort shared 400",
        "admin@saved-sort-400.test",
        "Admin",
        PW,
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role = 'admin' WHERE organization_id = $1 AND user_id = $2",
    )
    .bind(organization_id)
    .bind(admin_id)
    .execute(&migrator_pool)
    .await
    .unwrap();
    let member_id = create_user(&migrator_pool, "member@saved-sort-400.test", "Member", PW).await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;

    let app_pool = connect_as_app(&migrator_pool).await;
    let shared = create_list(
        &app_pool,
        organization_id,
        admin_id,
        Uuid::new_v4(),
        SavedListScope::Shared,
        "Shared 400 target",
        empty_filter(),
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let member_cookie = login_cookie(&router, "member@saved-sort-400.test", PW).await;
    let resp = put_json_with_cookie(
        &router,
        &format!("/api/saved-lists/{}", shared.list.id),
        &member_cookie,
        json!({
            "expected_revision": 1,
            "name": "Shared 400 target",
            "filter": {"version": 1, "clauses": []},
            "sort": "NAME.asc",
        }),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a malformed sort must fail decode before the 403 authorization check"
    );
    assert_eq!(body_json(resp).await, json!({"error": "malformed_request"}));
}

/// §11.9: `POST /api/saved-lists` with `"sort":"created.desc"` and a later
/// same-`request_id` replay with `sort` ABSENT over HTTP must be recognized
/// as the same fingerprint (both normalize to the default), returning 200
/// `created:false`, not a conflict.
#[sqlx::test]
#[ignore]
async fn saved_list_http_create_replay_with_absent_sort_matches_created_desc(
    migrator_pool: PgPool,
) {
    create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort HTTP replay",
        "owner@saved-sort-replay.test",
        "Owner",
        PW,
    )
    .await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-sort-replay.test", PW).await;
    let request_id = Uuid::new_v4();

    let first = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": request_id,
            "scope": "personal",
            "name": "Replay target",
            "filter": {"version": 1, "clauses": []},
            "sort": "created.desc",
        }),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = body_json(first).await;
    let id = first_body["list"]["id"].as_str().unwrap().to_string();

    let replay = post_json_with_cookie(
        &router,
        "/api/saved-lists",
        &cookie,
        json!({
            "request_id": request_id,
            "scope": "personal",
            "name": "Replay target",
            "filter": {"version": 1, "clauses": []},
        }),
    )
    .await;
    assert_eq!(replay.status(), StatusCode::OK);
    let replay_body = body_json(replay).await;
    assert_eq!(replay_body["created"], false);
    assert_eq!(replay_body["list"]["id"], id);
}

/// §5, §11.9: an absent `sort` on `UpdateSavedList` is the wire form of the
/// default order, exactly like `Some(PersonSort::DEFAULT)` — it normalizes
/// to a stored NULL pair and, when the previous sort was non-default, still
/// bumps the revision because the definition genuinely changed.
#[sqlx::test]
#[ignore]
async fn update_saved_list_with_absent_sort_normalizes_to_the_default_null_pair(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort absent default",
        "owner@saved-sort-absent.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let created = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Absent sort target",
        empty_filter(),
        Some(PersonSort::parse("name.asc").unwrap()),
    )
    .await;

    let updated = saved_list::update_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        UpdateSavedList {
            list_id: created.list.id,
            expected_revision: 1,
            name: "Absent sort target".to_string(),
            filter: empty_filter(),
            sort: None,
        },
    )
    .await
    .unwrap();
    assert!(
        updated.changed,
        "an absent sort differs from the stored name.asc"
    );
    assert_eq!(updated.list.revision, 2);

    let stored: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT sort_key, sort_direction FROM saved_list WHERE id = $1")
            .bind(created.list.id.as_uuid())
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
    assert_eq!(stored, (None, None));
}

/// §11.9 symmetric replay: the same retry token replays identically when
/// the sort is repeated verbatim (not just when both sides normalize to the
/// default, as the existing retry test proves), and conflicts when a later
/// call on the same token omits the sort — a genuinely different
/// fingerprint from the originally-stored explicit sort.
#[sqlx::test]
#[ignore]
async fn saved_list_create_retry_replays_identically_with_the_same_explicit_sort(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort symmetric retry",
        "owner@saved-sort-symmetric.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let request_id = Uuid::new_v4();
    let name_asc = PersonSort::parse("name.asc").unwrap();

    let created = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "Symmetric retry",
        empty_filter(),
        Some(name_asc),
    )
    .await;
    assert!(created.created);

    let replay = create_list_with_sort(
        &app_pool,
        organization_id,
        owner_id,
        request_id,
        SavedListScope::Personal,
        "Symmetric retry",
        empty_filter(),
        Some(name_asc),
    )
    .await;
    assert!(
        !replay.created,
        "the identical explicit sort must replay, not conflict"
    );
    assert_eq!(replay.list.id, created.list.id);

    let mismatched = saved_list::create_saved_list(
        &app_pool,
        &command_context(organization_id, owner_id),
        CreateSavedList {
            request_id,
            scope: SavedListScope::Personal,
            name: "Symmetric retry".to_string(),
            filter: empty_filter(),
            sort: None,
        },
    )
    .await;
    assert!(
        matches!(mismatched, Err(SavedListError::RequestConflict)),
        "an absent sort differs from the originally-stored name.asc fingerprint"
    );
}

/// §5, §11.9: a stored sort pair the binary cannot read (simulated skew,
/// per `unrecognized_stored_sort_pair_fails_closed_on_detail_and_count`) is
/// fully repaired by a normal owner PUT — with an explicit valid sort, or
/// with `sort: null` — because PUT always writes a fresh, valid definition
/// rather than patching around the corrupt row.
#[sqlx::test]
#[ignore]
async fn owner_put_repairs_an_unreadable_stored_sort_with_a_valid_sort_or_null(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort repair",
        "owner@saved-sort-repair.test",
        "Owner",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let router = build_router(&migrator_pool).await;
    let cookie = login_cookie(&router, "owner@saved-sort-repair.test", PW).await;

    sqlx::query("ALTER TABLE saved_list DROP CONSTRAINT saved_list_sort_key_check")
        .execute(&migrator_pool)
        .await
        .unwrap();

    for (label, repair_sort) in [
        ("valid sort", json!("name.asc")),
        ("null sort", Value::Null),
    ] {
        let created = create_list(
            &app_pool,
            organization_id,
            owner_id,
            Uuid::new_v4(),
            SavedListScope::Personal,
            &format!("Repair target ({label})"),
            empty_filter(),
        )
        .await;
        sqlx::query(
            "UPDATE saved_list SET sort_key = 'distance', sort_direction = 'asc' WHERE id = $1",
        )
        .bind(created.list.id.as_uuid())
        .execute(&migrator_pool)
        .await
        .unwrap();

        let repair_resp = put_json_with_cookie(
            &router,
            &format!("/api/saved-lists/{}", created.list.id),
            &cookie,
            json!({
                "expected_revision": 1,
                "name": format!("Repaired ({label})"),
                "filter": {"version": 1, "clauses": []},
                "sort": repair_sort,
            }),
        )
        .await;
        assert_eq!(repair_resp.status(), StatusCode::OK, "{label}");
        let repair_body = body_json(repair_resp).await;
        assert_eq!(repair_body["changed"], true, "{label}");

        let detail = body_json(
            get_with_cookie(
                &router,
                &format!("/api/saved-lists/{}", created.list.id),
                &cookie,
            )
            .await,
        )
        .await;
        assert_ne!(detail["filter"], Value::Null, "{label}");
        assert_eq!(detail["filter_error"], Value::Null, "{label}");
    }
}

/// §7, §11.9: a foreign Organization's list carrying a sort is still hidden
/// behind the identical 404 envelope as a same-Organization hidden personal
/// list — the sort must not change or leak through the privacy contract.
#[sqlx::test]
#[ignore]
async fn saved_list_http_hides_a_foreign_sorted_list_with_the_same_404_as_hidden_personal(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort privacy A",
        "owner@saved-sort-privacy.test",
        "Owner",
        PW,
    )
    .await;
    let member_id = create_user(
        &migrator_pool,
        "member@saved-sort-privacy.test",
        "Member",
        PW,
    )
    .await;
    add_membership_with(
        &migrator_pool,
        organization_id,
        member_id,
        Role::Member,
        MembershipStatus::Active,
    )
    .await;
    let (foreign_organization_id, foreign_owner_id) = create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort privacy B",
        "foreign@saved-sort-privacy.test",
        "Foreign",
        PW,
    )
    .await;
    let app_pool = connect_as_app(&migrator_pool).await;
    let personal = create_list(
        &app_pool,
        organization_id,
        owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Owner private (no sort)",
        empty_filter(),
    )
    .await;
    let foreign = create_list_with_sort(
        &app_pool,
        foreign_organization_id,
        foreign_owner_id,
        Uuid::new_v4(),
        SavedListScope::Personal,
        "Foreign private (sorted)",
        empty_filter(),
        Some(PersonSort::parse("name.asc").unwrap()),
    )
    .await;

    let router = build_router(&migrator_pool).await;
    let member_cookie = login_cookie(&router, "member@saved-sort-privacy.test", PW).await;

    const NOT_FOUND: &[u8] = br#"{"error":"not_found"}"#;
    let personal_resp = get_with_cookie(
        &router,
        &format!("/api/saved-lists/{}", personal.list.id),
        &member_cookie,
    )
    .await;
    assert_status_body(
        personal_resp,
        StatusCode::NOT_FOUND,
        NOT_FOUND,
        "hidden personal, no sort",
    )
    .await;

    let foreign_resp = get_with_cookie(
        &router,
        &format!("/api/saved-lists/{}", foreign.list.id),
        &member_cookie,
    )
    .await;
    assert_status_body(
        foreign_resp,
        StatusCode::NOT_FOUND,
        NOT_FOUND,
        "foreign, sorted",
    )
    .await;
}

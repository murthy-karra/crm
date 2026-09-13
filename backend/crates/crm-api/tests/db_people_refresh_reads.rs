//! Real retained captures and command-created plans exercise HTTP/read boundaries.
//! Privileged fixture writes change only authorization in the denial tests.
use crate::{common, db_activity_source, db_people_refresh_execution as execution, import_support};
use axum::{http::StatusCode, response::Response};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::{
        admin::{MembershipStatus, Role},
        envelope::CommandContext,
        migration::{people_refresh as refresh, people_refresh_worker, MigrationError},
    },
    ids::{OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

const ROOT: &str = "/api/migrations/fub/people-refreshes";

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn refresh_readiness_rejects_partial_schema(migrator: PgPool) {
    let readiness = ReleaseReadiness::for_tests();
    let mut connection = migrator.acquire().await.unwrap();
    readiness
        .require_people_refresh(&mut connection)
        .await
        .unwrap();
    for table in [
        "migration_people_refresh",
        "migration_people_refresh_plan",
        "migration_people_refresh_item",
        "migration_people_refresh_contact",
        "migration_people_refresh_result",
        "migration_people_refresh_baseline",
        "migration_people_refresh_receipt",
        "migration_people_refresh_reservation",
    ] {
        let mut tx = migrator.begin().await.unwrap();
        sqlx::query(&format!(
            "ALTER TABLE {table} RENAME TO synthetic_missing_refresh_table"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(
            readiness.require_people_refresh(&mut tx).await.is_err(),
            "missing {table} must block capability"
        );
        tx.rollback().await.unwrap();
    }
    for (table, column) in [
        ("migration_people_refresh", "confirmed_refresh_plan_id"),
        ("migration_people_refresh_plan", "prepared_bytes"),
    ] {
        let mut tx = migrator.begin().await.unwrap();
        sqlx::query(&format!(
            "ALTER TABLE {table} RENAME COLUMN {column} TO synthetic_missing_refresh_column"
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(
            readiness.require_people_refresh(&mut tx).await.is_err(),
            "missing {column} must block capability"
        );
        tx.rollback().await.unwrap();
    }
    readiness
        .require_people_refresh(&mut connection)
        .await
        .unwrap();
}

fn uuid(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().unwrap()).unwrap()
}
async fn checked(response: Response, status: StatusCode, maximum: usize) -> Value {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), maximum)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn reads_bind_authority_sealed_plan_and_opaque_cursor(migrator: PgPool) {
    let (f, parent, _) = execution::mapped_fixture(&migrator).await;
    let (run, _) = execution::ready(
        &f,
        parent,
        vec![
            json!({"id":101,"firstName":"Updated One"}),
            json!({"id":102,"firstName":"Updated Two"}),
        ],
    )
    .await;
    let page = checked(
        common::get_with_cookie(&f.app, &format!("{ROOT}/{run}/items?limit=1"), &f.cookie).await,
        StatusCode::OK,
        256 * 1024,
    )
    .await;
    let cursor = page["next_cursor"].as_str().unwrap();
    let item = uuid(&page["items"][0]["id"]);
    let routes = [
        format!("{ROOT}?parent_import_id={parent}"),
        format!("{ROOT}/{run}"),
        format!("{ROOT}/{run}/items"),
        format!("{ROOT}/{run}/items/{item}"),
        format!("{ROOT}/{run}/items/{item}/contacts"),
        format!("{ROOT}/{run}/items/{item}/fields/proposed/first_name"),
        format!("{ROOT}/{run}/results"),
    ];
    for route in &routes {
        checked(
            common::get_with_cookie(&f.app, route, &f.cookie).await,
            StatusCode::OK,
            256 * 1024,
        )
        .await;
        let denied = checked(
            common::get_with_cookie(&f.app, route, &f.member_cookie).await,
            StatusCode::FORBIDDEN,
            128 * 1024,
        )
        .await;
        assert!(!denied.to_string().contains("Updated"));
    }
    checked(
        common::get_with_cookie(&f.app, &format!("{ROOT}/{}", Uuid::new_v4()), &f.cookie).await,
        StatusCode::NOT_FOUND,
        128 * 1024,
    )
    .await;
    let foreign = common::create_org(&migrator, "Other refresh tenant").await;
    common::add_membership_with(
        &migrator,
        foreign,
        f.actor,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let other_org = CommandContext {
        organization_id: OrganizationId::new(foreign),
        ..f.ctx
    };
    assert!(matches!(
        refresh::detail(&f.pool, &f.key, &other_org, run).await,
        Err(MigrationError::NotFound)
    ));
    let other = common::create_user(
        &migrator,
        "second-refresh-reader@synthetic.test",
        "Other admin",
        "long synthetic password",
    )
    .await;
    common::add_membership_with(
        &migrator,
        f.org,
        other,
        Role::Admin,
        MembershipStatus::Active,
    )
    .await;
    let other_actor = CommandContext {
        actor_user_id: UserId::new(other),
        ..f.ctx
    };
    assert!(matches!(
        refresh::items(
            &f.pool,
            &f.key,
            &other_actor,
            run,
            refresh::Page {
                limit: Some(1),
                cursor: Some(cursor.into()),
                ..Default::default()
            }
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    for query in [
        format!("limit=2&cursor={cursor}"),
        format!("limit=1&disposition=eligible&cursor={cursor}"),
        "limit=51".into(),
        "disposition=arbitrary".into(),
    ] {
        checked(
            common::get_with_cookie(&f.app, &format!("{ROOT}/{run}/items?{query}"), &f.cookie)
                .await,
            StatusCode::BAD_REQUEST,
            128 * 1024,
        )
        .await;
    }
    checked(
        common::get_with_cookie(
            &f.app,
            &format!("{ROOT}/{run}/results?limit=1&cursor={cursor}"),
            &f.cookie,
        )
        .await,
        StatusCode::BAD_REQUEST,
        128 * 1024,
    )
    .await;
    let next = refresh::items(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        refresh::Page {
            limit: Some(1),
            cursor: Some(cursor.into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_ne!(next["items"][0]["id"], page["items"][0]["id"]);
    let detail = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
    refresh::repreview(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        refresh::RepreviewPeopleRefresh {
            request_id: Uuid::new_v4(),
            expected_plan_revision: detail["plan"]["revision"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        },
    )
    .await
    .unwrap();
    // The old sealed preview remains readable until the worker starts its successor.
    assert_eq!(
        refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap()["plan_id"],
        detail["plan"]["id"]
    );
    assert!(people_refresh_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    assert!(matches!(
        refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default()).await,
        Err(MigrationError::Conflict)
    ));
    for _ in 0..20 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["state"] == "ready" {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    assert!(matches!(
        refresh::items(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            refresh::Page {
                limit: Some(1),
                cursor: Some(cursor.into()),
                ..Default::default()
            }
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.actor).execute(&migrator).await.unwrap();
    checked(
        common::get_with_cookie(&f.app, &format!("{ROOT}/{run}/items/{item}"), &f.cookie).await,
        StatusCode::UNAUTHORIZED,
        128 * 1024,
    )
    .await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn retained_unicode_names_and_contacts_are_complete_bounded_and_scope_bound(
    migrator: PgPool,
) {
    let (f, parent, _) = execution::mapped_fixture(&migrator).await;
    let name = "Retained 👩🏽‍💼 <script>文字</script> ".repeat(1100);
    let emails = (0..81)
        .map(|i| json!({"value":format!("contact-{i}@synthetic.test")}))
        .collect::<Vec<_>>();
    let (run, detail) = execution::ready(
        &f,
        parent,
        vec![
            json!({"id":101,"firstName":name,"emails":emails}),
            json!({"id":102,"firstName":"Updated Two"}),
        ],
    )
    .await;
    let items = refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    assert!(
        !items.to_string().contains("<script>"),
        "summary must contain metadata only"
    );
    let item = uuid(
        &items["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["source_id"] == "101")
            .unwrap()["id"],
    );
    let scalar = checked(
        common::get_with_cookie(&f.app, &format!("{ROOT}/{run}/items/{item}"), &f.cookie).await,
        StatusCode::OK,
        128 * 1024,
    )
    .await;
    assert_eq!(
        scalar["proposed"]["truncated_fields"],
        json!(["first_name"])
    );
    assert!(scalar["proposed"]["first_name"].as_str().unwrap().len() <= 1024);
    let mut cursor: Option<String> = None;
    let mut assembled = String::new();
    let mut first_cursor = None;
    for _ in 0..100 {
        let mut route = format!("{ROOT}/{run}/items/{item}/fields/proposed/first_name?limit=1025");
        if let Some(token) = &cursor {
            route.push_str(&format!("&cursor={token}"));
        }
        let fragment = checked(
            common::get_with_cookie(&f.app, &route, &f.cookie).await,
            StatusCode::OK,
            128 * 1024,
        )
        .await;
        assert_eq!(fragment["plan_id"], detail["plan"]["id"]);
        assert_eq!(fragment["offset"], assembled.len().to_string());
        assert_eq!(fragment["total_bytes"], name.len().to_string());
        let part = fragment["fragment"].as_str().unwrap();
        assert!(part.len() <= 1025);
        assembled.push_str(part);
        cursor = fragment["next_cursor"].as_str().map(str::to_owned);
        if first_cursor.is_none() {
            first_cursor = cursor.clone();
        }
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(assembled, name);
    let token = first_cursor.unwrap();
    for suffix in [
        format!("baseline/first_name?limit=1025&cursor={token}"),
        format!("proposed/last_name?limit=1025&cursor={token}"),
        format!("proposed/first_name?limit=1026&cursor={token}"),
        "proposed/body?limit=1025".into(),
        "proposed/first_name?limit=3".into(),
    ] {
        checked(
            common::get_with_cookie(
                &f.app,
                &format!("{ROOT}/{run}/items/{item}/fields/{suffix}"),
                &f.cookie,
            )
            .await,
            StatusCode::BAD_REQUEST,
            128 * 1024,
        )
        .await;
    }
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    let mut proposed = Vec::new();
    let mut pages = 0;
    loop {
        let page = refresh::contacts(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            item,
            refresh::Page {
                cursor,
                limit: Some(17),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() <= 256 * 1024);
        pages += 1;
        assert!(pages < 100);
        for row in page["contacts"].as_array().unwrap() {
            assert!(seen.insert(uuid(&row["id"])));
            if row["side"] == "proposed" && row["kind"] == "email" {
                proposed.push(row["value"]["value"].as_str().unwrap().to_owned());
            }
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert!(pages > 1);
    proposed.sort();
    let mut expected = (0..81)
        .map(|i| format!("contact-{i}@synthetic.test"))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(proposed, expected);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn settlement_keeps_preview_immutable_and_results_traversal_fixed(migrator: PgPool) {
    let people=(101..107).map(|id|json!({"id":id,"firstName":format!("Before {id}"),"stage":"Lead","assignedUserId":3})).collect::<Vec<_>>();
    let f = import_support::fixture(&migrator, people).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let newer = (101..107)
        .map(|id| json!({"id":id,"firstName":format!("After {id}")}))
        .collect::<Vec<_>>();
    let (run, detail) = execution::ready(&f, parent, newer).await;
    let before = refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    let item_id = uuid(&before["items"][0]["id"]);
    let before_item = refresh::item(&f.pool, &f.key, &f.ctx, run, item_id)
        .await
        .unwrap();
    execution::confirm(&f, run, &execution::confirmation(&detail)).await;
    for _ in 0..20 {
        if refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap()["progress"]["settled_items"]
            .as_str()
            .unwrap()
            .parse::<usize>()
            .unwrap()
            >= 2
        {
            break;
        }
        assert!(people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests())
        )
        .await
        .unwrap());
    }
    let initial = refresh::results(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        refresh::Page {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut cursor = initial["next_cursor"].as_str().map(str::to_owned);
    assert!(cursor.is_some());
    execution::drain(&f, run).await;
    let mut bounded = initial["results"].as_array().unwrap().clone();
    while let Some(token) = cursor {
        let page = refresh::results(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            refresh::Page {
                limit: Some(1),
                cursor: Some(token),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        bounded.extend(page["results"].as_array().unwrap().clone());
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(
        bounded.len(),
        2,
        "later settlements must not enter an existing traversal"
    );
    let latest = refresh::results(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
        .await
        .unwrap();
    assert_eq!(latest["results"].as_array().unwrap().len(), 6);
    assert_eq!(
        refresh::items(&f.pool, &f.key, &f.ctx, run, refresh::Page::default())
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        refresh::item(&f.pool, &f.key, &f.ctx, run, item_id)
            .await
            .unwrap(),
        before_item
    );
}

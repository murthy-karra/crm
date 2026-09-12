//! TRUST/BOUNDARY/CONTRACT: bounded native reads on a real 010c review binding.
//! Native volume and zero-write cancelled-child rows are inert migrator fixtures,
//! not proof of activity source qualification or valid confirmation. The activity
//! command suites cover those writes; no worker reads this inert fixture data.
use crate::{
    common::{body_json, get_with_cookie},
    import_support::{self as support, Fixture},
};
use axum::{http::StatusCode, response::Response};
use crm_api::{
    auth::AuthContext,
    domain::{
        admin::Role,
        migration::{
            activity_review::{self, PageQuery, TaskState},
            imports::{self, AssigneeChoice, AssigneePatch, StageChoice, StagePatch},
        },
        person, task,
    },
    ids::{OrganizationId, PersonId, UserId},
};
use crm_app::auth::workspace;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub(super) async fn parent(migrator: &PgPool) -> (Fixture, Uuid, PersonId) {
    let f = support::fixture(
        migrator,
        vec![json!({"id":101,"firstName":"Synthetic review","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let (id, _) = support::propose(&f).await;
    support::drain_import(&f).await;
    support::replan(
        &f,
        id,
        "1",
        &[StagePatch {
            source_key: "4".into(),
            choice: StageChoice::Existing {
                stage_id: f.lead_stage,
            },
        }],
        &[AssigneePatch {
            source_key: "3".into(),
            choice: AssigneeChoice::Member { user_id: f.actor },
        }],
    )
    .await;
    support::drain_import(&f).await;
    let p = imports::detail(&f.pool, &f.key, &f.ctx, id, &f.policy)
        .await
        .unwrap();
    imports::confirm(&f.pool,&f.key,&f.ctx,id,serde_json::from_value(json!({"request_id":Uuid::new_v4(),"plan_id":p["plan"]["id"],
        "plan_revision":p["plan"]["revision"],"confirmation_digest":p["plan"]["confirmation_digest"],
        "acknowledgments":{"held_count":p["plan"]["counts"]["held_people"],"review_only":true,"remaining_data":true}})).unwrap(),
        &workspace::ReleaseReadiness::for_tests(),&f.policy).await.unwrap();
    support::drain_import(&f).await;
    let person:Uuid=sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND import_id=$2 AND family='people' AND source_id='101'")
        .bind(f.org).bind(id).fetch_one(&f.pool).await.unwrap();
    (f, id, PersonId::new(person))
}
fn auth(f: &Fixture) -> AuthContext {
    AuthContext {
        actor_user_id: UserId::new(f.actor),
        actor_email: "review@synthetic.test".into(),
        actor_display_name: "Synthetic admin".into(),
        active_organization_id: OrganizationId::new(f.org),
        active_organization_name: "Synthetic review workspace".into(),
        role: Role::Admin,
    }
}
async fn checked(response: Response, status: StatusCode) -> Value {
    assert_eq!(response.status(), status);
    if status == StatusCode::OK {
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    body_json(response).await
}
pub(super) async fn seed_activity(migrator: &PgPool, f: &Fixture, person: PersonId, n: i64) {
    sqlx::query("INSERT INTO note(organization_id,person_id,body,origin,correlation_id,created_at,updated_at) SELECT $1,$2,repeat('😀',10000),'migration',gen_random_uuid(),'2020-01-01Z'::timestamptz,'2020-01-01Z'::timestamptz FROM generate_series(1,$3)")
        .bind(f.org).bind(person.0).bind(n).execute(migrator).await.unwrap();
    sqlx::query("INSERT INTO task(organization_id,person_id,title,origin,correlation_id,due_at,completed_at,created_at,updated_at) SELECT $1,$2,'Synthetic task '||g,'migration',gen_random_uuid(),CASE WHEN g%3=0 THEN NULL ELSE '2021-01-01Z'::timestamptz END,CASE WHEN g%2=0 THEN '2021-01-02Z'::timestamptz ELSE NULL END,'2020-01-01Z'::timestamptz,'2020-01-01Z'::timestamptz FROM generate_series(1,120) g")
        .bind(f.org).bind(person.0).execute(migrator).await.unwrap();
}

#[sqlx::test]
#[ignore]
async fn native_review_pages_traverse_large_notes_and_timed_undated_tasks(migrator: PgPool) {
    let (f, _, person) = parent(&migrator).await;
    seed_activity(&migrator, &f, person, 501).await;
    let calls = f.reader.calls();
    let a = auth(&f);
    let detail = checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{person}/migration-review"),
            &f.cookie,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["activity"]["notes_count"], "501");
    assert_eq!(detail["activity"]["open_tasks_count"], "60");
    assert_eq!(detail["activity"]["completed_tasks_count"], "60");
    assert!(detail.get("history").is_none() && detail.get("tasks").is_none());
    assert!(detail["core_history"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["kind"] != "note" && v["kind"] != "task_completed"));
    let mut q = PageQuery {
        limit: Some(50),
        cursor: None,
    };
    let mut ids = Vec::new();
    loop {
        let p = activity_review::notes(&f.pool, &f.key, &a, person, &q)
            .await
            .unwrap();
        assert!(serde_json::to_vec(&p).unwrap().len() <= activity_review::PAGE_BYTES);
        assert!(p.items.len() <= 50);
        for item in &p.items {
            assert_eq!(item["excerpt"].as_str().unwrap().chars().count(), 512);
            assert_eq!(item["can_manage"], false);
            assert_eq!(item["has_more"], true);
            assert!(item.get("body").is_none());
            ids.push(Uuid::parse_str(item["id"].as_str().unwrap()).unwrap());
        }
        q.cursor = p.next_cursor;
        if q.cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 501);
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
    let full = checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{person}/migration-review/notes/{}", ids[0]),
            &f.cookie,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(full["body"].as_str().unwrap(), "😀".repeat(10000));
    assert_eq!(full["author"], Value::Null);
    assert_eq!(full["provenance"], Value::Null);
    for state in [TaskState::Open, TaskState::Completed] {
        let mut q = PageQuery {
            limit: Some(7),
            cursor: None,
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut undated = false;
        loop {
            let p = activity_review::tasks(&f.pool, &f.key, &a, person, state, &q)
                .await
                .unwrap();
            for item in &p.items {
                assert_eq!(item["can_manage"], false);
                assert!(seen.insert(item["id"].as_str().unwrap().to_owned()));
                if state == TaskState::Open {
                    if item["due_at"].is_null() {
                        undated = true;
                    } else {
                        assert!(!undated, "dated row after NULL cursor boundary");
                    }
                    assert!(item["completed_at"].is_null());
                } else {
                    assert!(!item["completed_at"].is_null());
                }
            }
            q.cursor = p.next_cursor;
            if q.cursor.is_none() {
                break;
            }
        }
        assert_eq!(seen.len(), 60);
    }
    assert_eq!(f.reader.calls(), calls, "native review is source-free");
}

#[sqlx::test]
#[ignore]
async fn review_requires_current_admin_real_binding_and_exact_person_scope(migrator: PgPool) {
    let (f, parent_id, person) = parent(&migrator).await;
    seed_activity(&migrator, &f, person, 2).await;
    let (foreign, _, foreign_person) = parent(&migrator).await;
    let base = format!("/api/people/{person}/migration-review");
    for suffix in ["", "/notes", "/tasks?state=open"] {
        checked(
            get_with_cookie(&f.app, &format!("{base}{suffix}"), &f.member_cookie).await,
            StatusCode::FORBIDDEN,
        )
        .await;
        checked(
            get_with_cookie(&f.app, &format!("{base}{suffix}"), "").await,
            StatusCode::UNAUTHORIZED,
        )
        .await;
    }
    checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{foreign_person}/migration-review"),
            &f.cookie,
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let own_note: Uuid = sqlx::query_scalar("SELECT id FROM note WHERE organization_id=$1 LIMIT 1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{foreign_person}/migration-review/notes/{own_note}"),
            &foreign.cookie,
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let p = activity_review::notes(
        &f.pool,
        &f.key,
        &auth(&f),
        person,
        &PageQuery {
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    let token = p.next_cursor.unwrap();
    for path in [
        format!("{base}/tasks?state=open&cursor={token}"),
        format!("{base}/notes?limit=51"),
        format!("{base}/notes?limit=0"),
        format!("{base}/notes?unknown=x"),
        format!("{base}/notes?cursor=bad"),
    ] {
        checked(
            get_with_cookie(&f.app, &path, &f.cookie).await,
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{foreign_person}/migration-review/notes?cursor={token}"),
            &foreign.cookie,
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    // Partially settled/cancelled People imports are still reviewable through
    // their real binding; eligibility to start the activity child is separate.
    sqlx::query("UPDATE migration_import SET state='cancelled' WHERE id=$1")
        .bind(parent_id)
        .execute(&migrator)
        .await
        .unwrap();
    checked(
        get_with_cookie(&f.app, &base, &f.cookie).await,
        StatusCode::OK,
    )
    .await;
    // Review mode without the real binding grants no access to these routes.
    sqlx::query("DELETE FROM migration_workspace WHERE organization_id=$1")
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    checked(
        get_with_cookie(&f.app, &base, &f.cookie).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let stale = auth(&f);
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(matches!(
        activity_review::detail(&f.pool, &stale, person).await,
        Err(activity_review::ReviewError::Forbidden)
    ));
}

// A deliberately inert zero-native-write child models a confirmed run that was
// then cancelled. Its encrypted placeholders are never decrypted or executed.
async fn inert_child(migrator: &PgPool, f: &Fixture, parent: Uuid) -> (Uuid, Uuid) {
    let child = Uuid::new_v4();
    let plan = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_activity_import(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,state) SELECT $1,i.organization_id,i.id,i.confirmed_plan_id,i.snapshot_id,i.preview_id,i.source_account_id,i.capture_sequence,o.workspace_revision,i.executor_user_id,'ready' FROM migration_import i JOIN organization o ON o.id=i.organization_id WHERE i.id=$2 AND i.organization_id=$3")
        .bind(child).bind(parent).bind(f.org).execute(migrator).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_plan(id,import_id,snapshot_id,organization_id,revision,state,patch_nonce,patch_ciphertext,destination_nonce,destination_ciphertext) VALUES($1,$2,$3,$4,1,'ready',''::bytea,''::bytea,''::bytea,''::bytea)")
        .bind(plan).bind(child).bind(f.snapshot).bind(f.org).execute(migrator).await.unwrap();
    (child, plan)
}

#[sqlx::test]
#[ignore]
async fn exclusive_boundary_serializes_legacy_read_and_survives_zero_write_cancel(
    migrator: PgPool,
) {
    let (f, parent, person) = parent(&migrator).await;
    let (child, plan) = inert_child(&migrator, &f, parent).await;
    let org = OrganizationId::new(f.org);
    let a = auth(&f);
    let mut legacy = f.pool.begin().await.unwrap();
    workspace::read_check(&mut legacy, org, a.actor_user_id, false)
        .await
        .unwrap();
    workspace::activity_complete_read(&mut legacy, org)
        .await
        .unwrap();
    let pool = migrator.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let mut writer = tokio::spawn(async move {
        let mut tx = pool.begin().await.unwrap();
        started.send(()).unwrap();
        workspace::exclusive(&mut tx, org).await.unwrap();
        sqlx::query("UPDATE migration_activity_import SET confirmed_plan_id=$1,state='cancelled' WHERE id=$2 AND organization_id=$3")
            .bind(plan).bind(child).bind(org.0).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    });
    ready.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(40), &mut writer)
            .await
            .is_err()
    );
    legacy.rollback().await.unwrap();
    writer.await.unwrap();
    let error = workspace::with_reader(&a, async {
        person::queries::history_for_person(&mut f.pool.acquire().await.unwrap(), org, person).await
    })
    .await
    .unwrap_err();
    assert!(workspace::is_activity_review_error(&error));
    let error = workspace::with_reader(&a, async {
        task::open_for_person(&mut f.pool.acquire().await.unwrap(), org, person).await
    })
    .await
    .unwrap_err();
    assert!(
        matches!(error,task::TaskError::Database(ref e) if workspace::is_activity_review_error(e))
    );
    let response = checked(
        get_with_cookie(&f.app, &format!("/api/people/{person}"), &f.cookie).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(response["error"], "activity_review_required");
    let response = checked(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{person}/migration-review"),
            &f.cookie,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(response["activity"]["notes_count"], "0");
    assert_eq!(response["activity"]["activity_revision"], "0");
}

#[sqlx::test]
#[ignore]
async fn native_page_cursor_requires_refresh_after_activity_revision_changes(migrator: PgPool) {
    let (f, parent, person) = parent(&migrator).await;
    seed_activity(&migrator, &f, person, 3).await;
    let (child, _) = inert_child(&migrator, &f, parent).await;
    let p = activity_review::notes(
        &f.pool,
        &f.key,
        &auth(&f),
        person,
        &PageQuery {
            limit: Some(1),
            cursor: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE migration_activity_import SET activity_revision=activity_revision+1 WHERE id=$1 AND organization_id=$2")
        .bind(child).bind(f.org).execute(&migrator).await.unwrap();
    let path = format!(
        "/api/people/{person}/migration-review/notes?limit=1&cursor={}",
        p.next_cursor.unwrap()
    );
    let error = checked(
        get_with_cookie(&f.app, &path, &f.cookie).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(error["error"], "activity_refresh_required");
    let p = activity_review::notes(&f.pool, &f.key, &auth(&f), person, &PageQuery::default())
        .await
        .unwrap();
    assert_eq!(p.activity_revision, "1");
    assert_eq!(p.items.len(), 3);
}

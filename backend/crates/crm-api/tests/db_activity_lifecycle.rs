//! TRUST/BOUNDARY/CONTRACT: real retained tasks, receipts, atomicity and byte ledgers.
use crate::{
    common::{body_json, get_with_cookie, post_json_with_cookie},
    db_activity_source as source,
    import_support::{self as support, Fixture},
};
use axum::http::StatusCode;
use crm_api::{
    auth::workspace,
    domain::migration::{
        activity::{self, ActivityAction, ActivityPage, Choice, MappingPatch},
        activity_worker, metadata, MigrationError,
    },
    ids::{OrganizationId, UserId},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub(super) async fn tasks(migrator: &PgPool, n: u64) -> (Fixture, Uuid, Uuid, Value) {
    let book = source::book();
    book.set_records(
        crm_api::domain::migration::snapshot_source::Stream::TasksOpen,
        (1..=n).map(source::task).collect(),
    );
    let f = support::fixture_with_book(migrator, book).await;
    let parent = source::completed_parent(&f).await;
    let (id, p) = source::prepare(&f, parent).await;
    let p = source::replan(&f, id, &p, source::choices(&f, id).await, None).await;
    (f, parent, id, p)
}
fn action(v: &Value) -> ActivityAction {
    ActivityAction {
        request_id: Uuid::new_v4(),
        expected_revision: v["revision"].as_str().unwrap().into(),
    }
}
pub(super) async fn assert_bytes(f: &Fixture, id: Uuid) {
    let row=sqlx::query("SELECT measured_bytes,retained_bytes,reserved_bytes FROM migration_activity_import WHERE id=$1 AND organization_id=$2").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(
        row.get::<i64, _>("measured_bytes"),
        row.get::<i64, _>("retained_bytes"),
        "all measured logical bytes settle exactly"
    );
    assert!(row.get::<i64, _>("reserved_bytes") >= 0);
    let actual:i64=sqlx::query_scalar("SELECT sum(retained_bytes)::bigint FROM migration_activity_import WHERE organization_id=$1").bind(f.org).fetch_one(&f.pool).await.unwrap();
    let ledger: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(ledger >= actual);
}

#[sqlx::test]
#[ignore]
async fn activity_http_scope_strict_inputs_receipt_replay_and_all_held_replanning(
    migrator: PgPool,
) {
    let (f, _, id, p) = tasks(&migrator, 1).await;
    let (other, _, otherid, _) = tasks(&migrator, 1).await;
    let root = format!("/api/migrations/fub/activity-imports/{id}");
    assert_eq!(
        get_with_cookie(&f.app, &root, &f.member_cookie)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get_with_cookie(
            &f.app,
            &format!("/api/migrations/fub/activity-imports/{otherid}"),
            &f.cookie
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let mut foreign = f.ctx.clone();
    foreign.organization_id = OrganizationId::new(other.org);
    assert!(matches!(
        activity::detail(&f.pool, &f.key, &foreign, id, &f.policy).await,
        Err(MigrationError::Forbidden)
    ));
    for hidden in [
        json!({"kind":"hold","target_id":f.actor}),
        json!({"kind":"leave_unmapped","user_id":f.actor}),
        json!({"kind":"map_kind","native_kind":"call","origin":"migration"}),
    ] {
        assert!(serde_json::from_value::<Choice>(hidden).is_err());
    }
    let maps = activity::mappings(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
        .await
        .unwrap();
    let first = Uuid::parse_str(maps["items"][0]["id"].as_str().unwrap()).unwrap();
    let held = source::replan(
        &f,
        id,
        &p,
        vec![MappingPatch {
            mapping_id: first,
            choice: Choice::Hold,
        }],
        None,
    )
    .await;
    assert_eq!(
        post_json_with_cookie(
            &f.app,
            &format!("{root}/confirm"),
            &f.cookie,
            json!(source::confirmation(&held))
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert!(source::ready(&f, id).await["confirmed_plan_id"].is_null());
    let p = source::replan(&f, id, &held, source::choices(&f, id).await, None).await;
    let command = json!(source::confirmation(&p));
    let original = post_json_with_cookie(
        &f.app,
        &format!("{root}/confirm"),
        &f.cookie,
        command.clone(),
    )
    .await;
    assert_eq!(original.status(), StatusCode::ACCEPTED);
    let original = body_json(original).await;
    sqlx::query("UPDATE migration_activity_plan SET expires_at=now()-interval '1 second' WHERE import_id=$1 AND organization_id=$2").bind(id).bind(f.org).execute(&migrator).await.unwrap();
    let replay = post_json_with_cookie(
        &f.app,
        &format!("{root}/confirm"),
        &f.cookie,
        command.clone(),
    )
    .await;
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    assert_eq!(body_json(replay).await, original);
    let mut changed = command.clone();
    changed["acknowledge_held"] = json!("999");
    assert_eq!(
        post_json_with_cookie(&f.app, &format!("{root}/confirm"), &f.cookie, changed)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        get_with_cookie(
            &f.app,
            &format!("/api/people/{}/migration-review", Uuid::new_v4()),
            &f.cookie
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    source::drain(&f).await;
    assert_bytes(&f, id).await;
    let page = activity::results(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
        .await
        .unwrap();
    let result = page["items"][0]["id"].as_str().unwrap();
    let response = get_with_cookie(
        &f.app,
        &format!("{root}/results/{result}/fields/all"),
        &f.cookie,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        get_with_cookie(
            &other.app,
            &format!("{root}/results/{result}/fields/all"),
            &other.cookie
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
#[ignore]
async fn activity_native_equal_local_edits_tombstone_legacy_and_identity_missing(migrator: PgPool) {
    let book = source::book();
    book.set_records(
        crm_api::domain::migration::snapshot_source::Stream::TasksOpen,
        (1..=5).map(source::task).collect(),
    );
    let f = support::fixture_with_book(&migrator, book).await;
    let parent = source::completed_parent(&f).await;
    let person:Uuid=sqlx::query_scalar("SELECT target_id FROM migration_import_identity WHERE organization_id=$1 AND import_id=$2 AND source_id='101'").bind(f.org).bind(parent).fetch_one(&f.pool).await.unwrap();
    for i in 1..=4 {
        let key = if i == 4 {
            "4".into()
        } else {
            format!("v1:17:{i}")
        };
        let title = if i == 2 {
            "Locally edited"
        } else if i == 3 {
            ""
        } else {
            "Call the client"
        };
        sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,assignee_user_id,created_by_user_id,origin,correlation_id,source,source_external_id,created_at,updated_at,deleted_at,deleted_by_user_id) VALUES($1,$2,$3,'call',$4,$5,'migration',gen_random_uuid(),'fub',$6,'2026-09-01T12:00:00Z','2026-09-01T12:00:00Z',CASE WHEN $7 THEN now() ELSE NULL END,CASE WHEN $7 THEN $5 ELSE NULL END)").bind(f.org).bind(person).bind(title).bind(f.member).bind(f.actor).bind(key).bind(i==3).execute(&migrator).await.unwrap();
    }
    let (id, p) = source::prepare(&f, parent).await;
    let plan = source::plan_id(&p);
    let manifest:Uuid=sqlx::query_scalar("SELECT id FROM migration_activity_manifest WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND source_id='5'").bind(id).bind(plan).bind(f.org).fetch_one(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,import_id,plan_id,manifest_id) VALUES($1,17,'task','5',$2,$3,$4,$5)").bind(f.org).bind(Uuid::new_v4()).bind(id).bind(plan).bind(manifest).execute(&migrator).await.unwrap();
    let p = source::replan(&f, id, &p, source::choices(&f, id).await, None).await;
    assert_eq!(p["counts"]["tasks"]["already_present"], "1");
    assert_eq!(p["counts"]["held_count"], "4");
    let done = source::confirm(&f, id, &p).await;
    assert_eq!(done["counts"]["tasks"]["already_present"], "1");
    assert_eq!(done["counts"]["tasks"]["applied"], "0");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM task WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(n, 4);
    assert_bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn activity_atomic_result_failure_rolls_back_native_identity_and_ledgers(migrator: PgPool) {
    let (f, parent, id, p) = tasks(&migrator, 1).await;
    let before = source::parent_state(&f, parent).await;
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        source::confirmation(&p),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE FUNCTION synthetic_activity_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='P0099',MESSAGE='synthetic atomic failure'; END $$; CREATE TRIGGER synthetic_activity_failure BEFORE INSERT ON migration_activity_result FOR EACH ROW EXECUTE FUNCTION synthetic_activity_failure();").execute(&migrator).await.unwrap();
    source::drain(&f).await;
    let paused = source::ready(&f, id).await;
    assert_eq!(paused["state"], "paused");
    for table in [
        "task",
        "migration_activity_identity",
        "migration_activity_result",
        "migration_activity_result_issue",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id=$1"
        ))
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "native/identity/result must share one transaction"
        )
    }
    sqlx::raw_sql("DROP TRIGGER synthetic_activity_failure ON migration_activity_result; DROP FUNCTION synthetic_activity_failure();").execute(&migrator).await.unwrap();
    activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        action(&paused),
        true,
        &f.policy,
    )
    .await
    .unwrap();
    source::drain(&f).await;
    assert_eq!(source::ready(&f, id).await["state"], "completed");
    assert_eq!(source::parent_state(&f, parent).await, before);
    assert_bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn activity_authority_loss_retry_adoption_and_partial_cancel_preserve_boundary(
    migrator: PgPool,
) {
    let (f, _, id, p) = tasks(&migrator, 3).await;
    let calls = f.reader.calls();
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        source::confirmation(&p),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    assert!(activity_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(activity_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task WHERE organization_id=$1")
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(matches!(
        activity::detail(&f.pool, &f.key, &f.ctx, id, &f.policy).await,
        Err(MigrationError::Forbidden)
    ));
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(&migrator)
    .await
    .unwrap();
    let mut ctx = f.ctx.clone();
    ctx.actor_user_id = UserId::new(f.member);
    let paused = activity::detail(&f.pool, &f.key, &ctx, id, &f.policy)
        .await
        .unwrap();
    assert_eq!(paused["pause_reason"], "authority_changed");
    activity::action(&f.pool, &f.key, &ctx, id, action(&paused), true, &f.policy)
        .await
        .unwrap();
    assert!(activity_worker::run_once(&f.pool, &f.key, &f.policy)
        .await
        .unwrap());
    let running = activity::detail(&f.pool, &f.key, &ctx, id, &f.policy)
        .await
        .unwrap();
    let request = action(&running);
    let request_json = json!(request);
    let receipt = activity::action(&f.pool, &f.key, &ctx, id, request, false, &f.policy)
        .await
        .unwrap();
    source::drain(&f).await;
    assert_eq!(
        activity::action(
            &f.pool,
            &f.key,
            &ctx,
            id,
            serde_json::from_value(request_json).unwrap(),
            false,
            &f.policy
        )
        .await
        .unwrap(),
        receipt
    );
    let row=sqlx::query("SELECT state,confirmed_plan_id,executor_user_id FROM migration_activity_import WHERE id=$1 AND organization_id=$2").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert_eq!(row.get::<String, _>("state"), "cancelled");
    assert!(row.get::<Option<Uuid>, _>("confirmed_plan_id").is_some());
    assert_eq!(row.get::<Uuid, _>("executor_user_id"), f.member);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(f.reader.calls(), calls);
    assert_bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn activity_private_db_permit_rejects_origin_token_and_wrong_target(migrator: PgPool) {
    let (f, _, id, p) = tasks(&migrator, 1).await;
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        source::confirmation(&p),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    let manifest=sqlx::query("SELECT id,person_id,target_id,native_source_key,creator_user_id,assignee_user_id FROM migration_activity_manifest WHERE import_id=$1 AND organization_id=$2 AND disposition='eligible'").bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE migration_activity_import SET state='running',lease_token=$3,lease_expires_at=now()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(id).bind(f.org).bind(lease).execute(&migrator).await.unwrap();
    for scenario in [
        "no_token",
        "bad_token",
        "wrong_target",
        "wrong_source",
        "wrong_actor",
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        if scenario != "no_token" {
            sqlx::query("SELECT set_config('crm.activity_token',$1,true),set_config('crm.activity_unit',$2,true)").bind(if scenario=="bad_token"{Uuid::new_v4()}else{lease}.to_string()).bind(manifest.get::<Uuid,_>("id").to_string()).execute(&mut *tx).await.unwrap();
        }
        let target = if scenario == "wrong_target" {
            Uuid::new_v4()
        } else {
            manifest.get("target_id")
        };
        let source = if scenario == "wrong_source" {
            "v1:99:1".into()
        } else {
            manifest.get::<String, _>("native_source_key")
        };
        let creator = if scenario == "wrong_actor" {
            None
        } else {
            manifest.get::<Option<Uuid>, _>("creator_user_id")
        };
        let result=sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,assignee_user_id,created_by_user_id,origin,correlation_id,source,source_external_id) VALUES($1,$2,$3,'Synthetic rejected','call',$4,$5,'migration',$6,'fub',$7)").bind(target).bind(f.org).bind(manifest.get::<Uuid,_>("person_id")).bind(manifest.get::<Option<Uuid>,_>("assignee_user_id")).bind(creator).bind(id).bind(source).execute(&mut *tx).await;
        assert_eq!(
            result
                .unwrap_err()
                .as_database_error()
                .and_then(|e| e.code())
                .as_deref(),
            Some("P010C")
        );
        tx.rollback().await.unwrap();
    }
    sqlx::query("UPDATE migration_activity_import SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(f.org).execute(&migrator).await.unwrap();
    source::drain(&f).await;
    assert_eq!(source::ready(&f, id).await["state"], "completed");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        1
    );

    // Independent role/tenant enforcement at the persistence boundary.
    for table in [
        "migration_activity_choice",
        "migration_activity_source",
        "migration_activity_manifest",
        "migration_activity_manifest_issue",
        "migration_activity_identity",
        "migration_activity_result",
        "migration_activity_result_issue",
        "migration_activity_receipt",
    ] {
        let sql = format!("DELETE FROM {table} WHERE false");
        let error = sqlx::query(&sql).execute(&f.pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501")
        );
    }
    assert!(sqlx::query_scalar::<_,bool>("SELECT has_column_privilege('crm_app','migration_activity_reservation','byte_count','UPDATE') AND NOT has_column_privilege('crm_app','migration_activity_reservation','organization_id','UPDATE')").fetch_one(&f.pool).await.unwrap());
    let foreign_org = crate::common::create_org(&migrator, "Synthetic foreign activity FK").await;
    let error=sqlx::query("INSERT INTO migration_activity_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,stream,representation,semantic_hmac,nonce,ciphertext) SELECT gen_random_uuid(),plan_id,import_id,snapshot_id,$2,family,source_id,record_id,capture_id,capture_sequence,ordinal,stream,representation,semantic_hmac,nonce,ciphertext FROM migration_activity_source WHERE import_id=$1 LIMIT 1")
        .bind(id).bind(foreign_org).execute(&f.pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23503")
    );
}

#[sqlx::test]
#[ignore]
async fn activity_storage_pause_and_sibling_cancel_keep_independent_reservations(migrator: PgPool) {
    let (f, parent, id, p) = tasks(&migrator, 1).await;
    let sibling = metadata::propose(
        &f.pool,
        &f.key,
        &f.ctx,
        metadata::PlanMetadataImport {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
        },
        &f.policy,
    )
    .await
    .unwrap();
    let sibling_id = Uuid::parse_str(sibling["import"]["id"].as_str().unwrap()).unwrap();
    let sibling_before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM migration_metadata_import i WHERE id=$1 AND organization_id=$2",
    )
    .bind(sibling_id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        source::confirmation(&p),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    let mut lower = f.policy.clone();
    lower.run_ceiling_bytes=sqlx::query_scalar("SELECT retained_bytes+reserved_bytes+1 FROM migration_snapshot WHERE id=$1 AND organization_id=$2").bind(f.snapshot).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert!(activity_worker::run_once(&f.pool, &f.key, &lower)
        .await
        .unwrap());
    let paused = source::ready(&f, id).await;
    assert_eq!(paused["state"], "paused");
    assert_eq!(paused["pause_reason"], "storage_limit");
    activity::action(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        action(&paused),
        false,
        &f.policy,
    )
    .await
    .unwrap();
    let sibling_after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM migration_metadata_import i WHERE id=$1 AND organization_id=$2",
    )
    .bind(sibling_id)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(sibling_before, sibling_after);
    let reserved: i64 = sqlx::query_scalar(
        "SELECT reserved_bytes FROM migration_snapshot WHERE id=$1 AND organization_id=$2",
    )
    .bind(f.snapshot)
    .bind(f.org)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(reserved, sibling_after["reserved_bytes"].as_i64().unwrap());
    assert_bytes(&f, id).await;
}

#[sqlx::test]
#[ignore]
async fn actual_activity_confirmation_waits_for_reader_and_overlapping_workers_settle_once(
    migrator: PgPool,
) {
    let (f, _, id, p) = tasks(&migrator, 2).await;
    let mut reader = f.pool.begin().await.unwrap();
    workspace::read_check(
        &mut reader,
        f.ctx.organization_id,
        f.ctx.actor_user_id,
        false,
    )
    .await
    .unwrap();
    sqlx::query("SELECT crm_activity_complete_read($1)")
        .bind(f.org)
        .execute(&mut *reader)
        .await
        .unwrap();
    let (pool, key, ctx, policy) = (
        f.pool.clone(),
        f.key.clone(),
        f.ctx.clone(),
        f.policy.clone(),
    );
    let cmd = source::confirmation(&p);
    let mut confirm = tokio::spawn(async move {
        activity::confirm(
            &pool,
            &key,
            &ctx,
            id,
            cmd,
            &workspace::ReleaseReadiness::for_tests(),
            &policy,
        )
        .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut confirm)
            .await
            .is_err(),
        "first confirmation must wait for shared reader"
    );
    reader.rollback().await.unwrap();
    confirm.await.unwrap().unwrap();
    let err = sqlx::query("SELECT crm_activity_complete_read($1)")
        .bind(f.org)
        .execute(&f.pool)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("P010F")
    );
    let (a, b) = tokio::join!(
        activity_worker::run_once(&f.pool, &f.key, &f.policy),
        activity_worker::run_once(&f.pool, &f.key, &f.policy)
    );
    a.unwrap();
    b.unwrap();
    source::drain(&f).await;
    for table in [
        "task",
        "migration_activity_identity",
        "migration_activity_result",
        "migration_activity_result_issue",
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id=$1"
        ))
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap();
        assert_eq!(
            count,
            if table == "migration_activity_result_issue" {
                0
            } else {
                2
            },
            "successful tasks have native/identity/result rows and no result issues: {table}"
        )
    }
    assert_bytes(&f, id).await;
    let rows = activity::records(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
        .await
        .unwrap();
    let record = Uuid::parse_str(rows["items"][0]["id"].as_str().unwrap()).unwrap();
    let observations =
        activity::observations(&f.pool, &f.key, &f.ctx, id, record, ActivityPage::default())
            .await
            .unwrap();
    let observation = Uuid::parse_str(observations["items"][0]["id"].as_str().unwrap()).unwrap();
    let mut query = activity::FieldQuery {
        cursor: None,
        limit: Some(4),
        plan_id: None,
    };
    let mut raw = String::new();
    loop {
        let segment = activity::field(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            "observations",
            observation,
            "raw",
            query,
        )
        .await
        .unwrap();
        raw.push_str(segment["text"].as_str().unwrap());
        let Some(cursor) = segment["next_cursor"].as_str() else {
            break;
        };
        query = activity::FieldQuery {
            cursor: Some(cursor.into()),
            limit: Some(4),
            plan_id: None,
        };
    }
    let capture: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(capture["tasks"].as_array().unwrap().len(), 2);
}

#[sqlx::test]
#[ignore]
async fn expired_plan_and_real_release_report_reject_new_confirmation(migrator: PgPool) {
    use chrono::{Duration, Utc};
    let (f, _, id, p) = tasks(&migrator, 1).await;
    sqlx::query("UPDATE migration_activity_plan SET expires_at=now()-interval '1 second' WHERE import_id=$1 AND organization_id=$2")
        .bind(id).bind(f.org).execute(&migrator).await.unwrap();
    let rejected = activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        source::confirmation(&p),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await;
    assert!(matches!(rejected, Err(MigrationError::ImportExpired)));
    assert!(source::ready(&f, id).await["confirmed_plan_id"].is_null());
    let fresh = source::replan(&f, id, &p, Vec::new(), None).await;
    let hash = workspace::artifact_fingerprint().await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let path = std::env::temp_dir().join(format!("activity-release-{}.json", Uuid::new_v4()));
    let report = json!({"confirmation_ready":true,"metadata_confirmation_ready":true,"activity_confirmation_ready":true,"database_name":database,"checked_at":Utc::now(),"evidence_expires_at":Utc::now()+Duration::milliseconds(250),"candidates":[{"sha256":hash,"gate_version":workspace::GATE_VERSION,"capabilities":["fub-activity-import-v1"]}]});
    std::fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();
    let release = workspace::ReleaseReadiness::load_report(&f.pool, &path)
        .await
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(release.activity_ready());
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!release.activity_ready());
    assert!(matches!(
        activity::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            source::confirmation(&fresh),
            &release,
            &f.policy
        )
        .await,
        Err(MigrationError::ReleaseNotReady)
    ));
    assert!(source::ready(&f, id).await["confirmed_plan_id"].is_null());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn activity_logs_have_positive_phase_controls_without_source_content(migrator: PgPool) {
    use crm_api::domain::migration::snapshot_source::Stream;
    use std::sync::{Arc, Mutex};
    use tracing::instrument::WithSubscriber;
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }
    let book = source::book();
    let title = "TASK_CONTENT_SENTINEL_010F2";
    let kind = "TYPE_CONTENT_SENTINEL_010F2".repeat(8000);
    let note = "NOTE_CONTENT_SENTINEL_010F2";
    let mut t = source::task(1);
    t["name"] = json!(title);
    t["type"] = json!(kind);
    book.set_records(Stream::TasksOpen, vec![t]);
    let raw = json!({"id":2,"personId":101,"userId":3,"isHtml":true,"body":format!("<p>{note}</p>"),"created":"2026-09-01T12:00:00Z","updated":null});
    book.set_records(Stream::Notes, vec![raw.clone()]);
    book.set_raw(
        Stream::NoteDetail,
        0,
        200,
        serde_json::to_vec(&raw).unwrap(),
        false,
    );
    let f = support::fixture_with_book(&migrator, book).await;
    let parent = source::completed_parent(&f).await;
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .without_time()
        .with_writer(capture.clone())
        .finish();
    let id = async {
        let (id, p) = source::prepare(&f, parent).await;
        let maps = activity::mappings(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
            .await
            .unwrap();
        let choices = maps["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| MappingPatch {
                mapping_id: Uuid::parse_str(row["id"].as_str().unwrap()).unwrap(),
                choice: if row["role"] == "task_kind" {
                    Choice::MapKind {
                        native_kind: "other".into(),
                    }
                } else {
                    Choice::MapExisting { target_id: f.actor }
                },
            })
            .collect();
        let p = source::replan(&f, id, &p, choices, None).await;
        let page = activity::records(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
            .await
            .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() < 512 * 1024);
        let task = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "task")
            .unwrap();
        assert_eq!(task["preview"]["native"]["source_type_abbreviated"], true);
        assert_eq!(
            task["preview"]["native"]["source_type_full_utf8_bytes"],
            kind.len().to_string()
        );
        source::confirm(&f, id, &p).await;
        id
    }
    .with_subscriber(subscriber)
    .await;
    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("Activity import unit committed"));
    assert!(logs.contains(&id.to_string()));
    assert!(logs.contains("phase="));
    for sentinel in [
        title,
        "TYPE_CONTENT_SENTINEL_010F2",
        note,
        "Synthetic source user",
        "source-user@synthetic.test",
    ] {
        assert!(!logs.contains(sentinel), "source value leaked");
    }
    let tasks: Vec<String> = sqlx::query_scalar("SELECT title FROM task WHERE organization_id=$1")
        .bind(f.org)
        .fetch_all(&f.pool)
        .await
        .unwrap();
    assert!(
        tasks.contains(&title.to_owned()),
        "positive native task content control"
    );
    let bodies: Vec<String> = sqlx::query_scalar("SELECT body FROM note WHERE organization_id=$1")
        .bind(f.org)
        .fetch_all(&f.pool)
        .await
        .unwrap();
    assert!(
        bodies.iter().any(|body| body.contains(note)),
        "positive native note content control"
    );
}

#[sqlx::test]
#[ignore]
async fn confirmed_activity_can_complete_with_zero_inserts_after_revalidation(migrator: PgPool) {
    let (f, parent, id, p) = tasks(&migrator, 1).await;
    let parent_before = source::parent_state(&f, parent).await;
    activity::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        source::confirmation(&p),
        &workspace::ReleaseReadiness::for_tests(),
        &f.policy,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2")
        .bind(f.org).bind(f.member).execute(&migrator).await.unwrap();
    source::drain(&f).await;
    let done = source::ready(&f, id).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["counts"]["tasks"]["applied"], "0");
    assert_eq!(done["counts"]["tasks"]["held"], "1");
    assert!(!done["confirmed_plan_id"].is_null());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    let result = activity::results(&f.pool, &f.key, &f.ctx, id, ActivityPage::default())
        .await
        .unwrap();
    assert!(result["items"][0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "mapping_target_changed"));
    // The immutable plan remains eligible. Execution changed its disposition,
    // so result filtering must use committed reasons rather than plan issues.
    let issue = || ActivityPage {
        issue: Some("mapping_target_changed".into()),
        ..Default::default()
    };
    let planned = activity::records(&f.pool, &f.key, &f.ctx, id, issue())
        .await
        .unwrap();
    assert!(planned["items"].as_array().unwrap().is_empty());
    let filtered = activity::results(&f.pool, &f.key, &f.ctx, id, issue())
        .await
        .unwrap();
    assert_eq!(filtered["items"], result["items"]);
    let absent = activity::results(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        ActivityPage {
            issue: Some("native_local_changes".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(absent["items"].as_array().unwrap().is_empty());
    let codes:Vec<String> = sqlx::query_scalar("SELECT code FROM migration_activity_result_issue WHERE import_id=$1 AND organization_id=$2 ORDER BY code")
        .bind(id).bind(f.org).fetch_all(&f.pool).await.unwrap();
    let mut actual: Vec<String> = result["items"][0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    actual.sort();
    actual.dedup();
    assert_eq!(codes, actual);
    let measured:i64=sqlx::query_scalar("SELECT sum(crm_activity_retained_size('migration_activity_result_issue',to_jsonb(x)))::bigint FROM migration_activity_result_issue x WHERE import_id=$1 AND organization_id=$2")
        .bind(id).bind(f.org).fetch_one(&f.pool).await.unwrap();
    assert!(measured >= 256 + "mapping_target_changed".len() as i64);
    let foreign_org = crate::common::create_org(&migrator, "Synthetic foreign result issue").await;
    let error=sqlx::query("INSERT INTO migration_activity_result_issue(result_id,plan_id,import_id,organization_id,code) SELECT id,plan_id,import_id,$2,'foreign_issue' FROM migration_activity_result WHERE import_id=$1 LIMIT 1")
        .bind(id).bind(foreign_org).execute(&f.pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23503")
    );
    assert_eq!(source::parent_state(&f, parent).await, parent_before);
    assert_bytes(&f, id).await;
    let mut read = f.pool.begin().await.unwrap();
    workspace::shared(&mut read, f.ctx.organization_id)
        .await
        .unwrap();
    assert!(workspace::is_activity_review_error(
        &workspace::activity_complete_read(&mut read, f.ctx.organization_id)
            .await
            .unwrap_err()
    ));
}

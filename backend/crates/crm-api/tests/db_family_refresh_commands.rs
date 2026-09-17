//! Real typed preparation admission. No refresh confirmation or native execution.
use crate::{
    db_activity_source, db_history_capture_support as capture,
    db_history_import_support as history, db_people_admission_execution as admission,
    import_support,
};
use crm_api::{
    domain::migration::{
        family_refresh::{
            commands::{self, PrepareFamilyRefresh},
            evidence::{Purpose, Scope},
            model::Family,
            preparation_worker::{self as worker, Progress},
        },
        MigrationError,
    },
    ids::{OrganizationId, UserId},
};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_prepare_combined_is_atomic_metered_and_replay_safe(pool: PgPool) {
    let (f, parent, _, book) = history::fixture(&pool).await;
    let report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let (history, _) = capture::propose(&f, parent).await;
    capture::confirm(&f, history).await;
    capture::drain(&f, &book).await;
    let request = Uuid::new_v4();
    let input = || PrepareFamilyRefresh {
        request_id: request,
        parent_import_id: parent,
        core_report_id: Some(report),
        history_capture_id: Some(history),
        families: vec![Family::History, Family::Activity, Family::Metadata],
    };
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        commands::prepare(
            &f.pool,
            &f.key,
            &tiny,
            &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
            &f.ctx,
            input()
        )
        .await,
        Err(MigrationError::StorageLimit)
    ));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_bundle WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "failed admission leaves no bundle or receipt");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_metadata_catalog_readiness WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "failed refresh admission also rolls back the handover"
    );
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        input(),
    )
    .await
    .unwrap();
    assert!(sqlx::query_scalar::<_,bool>("SELECT state='ready' AND engine_version='fub-admitted-metadata-v1' FROM migration_metadata_catalog_readiness WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap(), "typed admission prepares the shared catalog without a separate admitted import");
    assert_eq!(prepared.families.len(), 3);
    assert_eq!(prepared.families[0].family, Family::Metadata);
    assert_eq!(prepared.state, "preparing");
    assert_eq!(prepared.revision, "1");
    let summary = crm_api::domain::migration::family_refresh::queries::detail(
        &f.pool,
        &f.ctx,
        prepared.bundle_id,
    )
    .await
    .unwrap();
    assert_eq!(
        summary
            .families
            .iter()
            .map(|f| f.family)
            .collect::<Vec<_>>(),
        vec![Family::Metadata, Family::Activity, Family::History]
    );
    assert!(summary
        .families
        .iter()
        .all(|f| f.counts.units == 0 && f.revision == "1"));

    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: prepared.bundle_id,
        plan: prepared.families[0].plan_id,
        family: Family::Metadata,
        revision: 1,
    };
    let b = sqlx::query("SELECT * FROM migration_family_refresh_bundle WHERE id=$1")
        .bind(prepared.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let binding: serde_json::Value = scope
        .open(
            &f.key,
            prepared.bundle_id,
            Purpose::Binding,
            b.get("source_nonce"),
            b.get("source_ciphertext"),
        )
        .unwrap();
    assert_eq!(binding["history"]["capture_id"], json!(history));
    assert_eq!(binding["core_report_id"], json!(report));
    let rows=sqlx::query("SELECT measured_bytes,retained_bytes,reserved_bytes,phase FROM migration_family_refresh_plan WHERE bundle_id=$1").bind(prepared.bundle_id).fetch_all(&pool).await.unwrap();
    for row in rows {
        assert_eq!(
            row.get::<i64, _>("retained_bytes"),
            row.get::<i64, _>("measured_bytes")
        );
        assert_eq!(row.get::<i64, _>("reserved_bytes"), 8192);
        assert_eq!(row.get::<String, _>("phase"), "cohort");
    }
    assert_eq!(
        b.get::<i64, _>("shared_retained_bytes"),
        b.get::<i64, _>("shared_measured_bytes")
    );
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    let replay = commands::prepare(
        &f.pool,
        &f.key,
        &tiny,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        input(),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
    let replay_without_readiness =
        commands::prepare_with_readiness(&f.pool, &f.key, &tiny, None, &f.ctx, input())
            .await
            .unwrap();
    assert_eq!(
        serde_json::to_value(replay_without_readiness).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
    let root = "/api/migrations/fub/family-refreshes";
    let response = crate::common::post_json_with_cookie(
        &f.app,
        root,
        &f.cookie,
        serde_json::to_value(input()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        crate::common::body_json(response).await,
        serde_json::to_value(&prepared).unwrap()
    );
    let id = prepared.bundle_id;
    let plan = prepared.families[0].plan_id;
    for route in [
        format!("{root}?parent_import_id={parent}"),
        format!("{root}/{id}"),
        format!("{root}/{id}/families"),
        format!("{root}/{id}/items?family=metadata&plan_id={plan}"),
        format!("{root}/{id}/mappings?family=metadata&plan_id={plan}"),
        format!("{root}/{id}/results?family=metadata&plan_id={plan}"),
    ] {
        for (cookie, status) in [
            (&f.cookie, axum::http::StatusCode::OK),
            (&f.member_cookie, axum::http::StatusCode::FORBIDDEN),
        ] {
            let response = crate::common::get_with_cookie(&f.app, &route, cookie).await;
            assert_eq!(response.status(), status, "{route}");
            assert_eq!(response.headers()["cache-control"], "no-store");
        }
    }
    for endpoint in ["plans", "confirm", "resume", "cancel"] {
        let route = format!("{root}/{id}/{endpoint}");
        let response = crate::common::post_json_with_cookie(
            &f.app,
            &route,
            &f.cookie,
            json!({"untrusted_role":"admin"}),
        )
        .await;
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM migration_snapshot_storage s WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let mut changed = input();
    changed.families.swap(0, 1);
    assert!(matches!(
        commands::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
            &f.ctx,
            changed
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let mut second = input();
    second.request_id = Uuid::new_v4();
    assert!(matches!(
        commands::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
            &f.ctx,
            second
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let mut stranger = f.ctx.clone();
    stranger.actor_user_id = UserId::new(Uuid::new_v4());
    assert!(matches!(
        commands::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
            &stranger,
            input()
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    stranger = f.ctx.clone();
    stranger.organization_id = OrganizationId::new(Uuid::new_v4());
    assert!(commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &stranger,
        input()
    )
    .await
    .is_err());
    let installed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_family_refresh_requirement WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        installed, 0,
        "preparation does not activate native refresh capability"
    );
    let first = worker::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(first.plan, prepared.families[0].plan_id);
    assert!(worker::claim_next(&f.pool).await.unwrap().is_none());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(first.token.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_expires_at=clock_timestamp()+interval '100 milliseconds' WHERE id=$1")
        .bind(first.plan).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let successor = worker::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(successor.plan, first.plan);
    assert_eq!(successor.epoch, first.epoch + 1);
    assert_ne!(successor.token, first.token);
    assert!(!worker::release(&f.pool, &first).await.unwrap());
    assert!(worker::release(&f.pool, &successor).await.unwrap());
    let mut idle = false;
    for _ in 0..100 {
        match worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() {
            Progress::Advanced => {}
            Progress::Idle => {
                idle = true;
                break;
            }
            Progress::Paused => panic!("valid retained sources should advance"),
        }
    }
    assert!(idle, "bounded preparation should exhaust runnable phases");
    let plans=sqlx::query("SELECT family,source_walk_complete,owned_walk_complete,mappings_complete,phase,lease_token,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE bundle_id=$1")
        .bind(prepared.bundle_id).fetch_all(&pool).await.unwrap();
    for plan in plans {
        let history = plan.get::<String, _>("family") == "history";
        let walked = true;
        assert_eq!(plan.get::<String, _>("phase"), "classify");
        assert_eq!(plan.get::<bool, _>("source_walk_complete"), walked);
        assert_eq!(plan.get::<bool, _>("owned_walk_complete"), walked);
        assert_eq!(plan.get::<bool, _>("mappings_complete"), !history);
        assert!(plan.get::<Option<Uuid>, _>("lease_token").is_none());
        assert_eq!(
            plan.get::<i64, _>("measured_bytes"),
            plan.get::<i64, _>("retained_bytes")
        );
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_prepare_history_only_uses_history_payer(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    assert_eq!(prepared.families.len(), 1);
    let row=sqlx::query("SELECT b.payer_plan_id,b.core_snapshot_id,p.history_capture_id,p.retained_bytes,p.reserved_bytes FROM migration_family_refresh_bundle b JOIN migration_family_refresh_plan p ON p.bundle_id=b.id WHERE b.id=$1").bind(prepared.bundle_id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        row.get::<Uuid, _>("payer_plan_id"),
        prepared.families[0].plan_id
    );
    assert!(row.get::<Option<Uuid>, _>("core_snapshot_id").is_none());
    assert_eq!(row.get::<Uuid, _>("history_capture_id"), history);
    assert_eq!(row.get::<i64, _>("reserved_bytes"), 8192);
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_prepare_rejects_invalid_selection_and_foreign_evidence(pool: PgPool) {
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let input = || PrepareFamilyRefresh {
        request_id: Uuid::new_v4(),
        parent_import_id: parent,
        core_report_id: Some(Uuid::new_v4()),
        history_capture_id: None,
        families: vec![Family::Activity],
    };
    assert!(matches!(
        commands::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
            &f.ctx,
            input()
        )
        .await,
        Err(MigrationError::SourceNotEligible)
    ));
    let other = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let other_parent = db_activity_source::completed_parent(&other).await;
    let foreign_report = admission::report(
        &other,
        other_parent,
        vec![json!({"id":101,"firstName":"Foreign","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let mut foreign = input();
    foreign.core_report_id = Some(foreign_report);
    assert!(matches!(
        commands::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
            &f.ctx,
            foreign
        )
        .await,
        Err(MigrationError::SourceNotEligible)
    ));
    for families in [
        vec![],
        vec![Family::Activity, Family::Activity],
        vec![Family::History],
    ] {
        let mut cmd = input();
        cmd.families = families;
        assert!(matches!(
            commands::prepare(
                &f.pool,
                &f.key,
                &f.policy,
                &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
                &f.ctx,
                cmd
            )
            .await,
            Err(MigrationError::InvalidInput)
        ));
    }
    let mut json = serde_json::to_value(input()).unwrap();
    json["organization_id"] = json!(f.org);
    assert!(serde_json::from_value::<PrepareFamilyRefresh>(json).is_err());
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_worker_pauses_without_losing_control_capacity(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &tiny).await.unwrap(),
        Progress::Paused
    );
    let row = sqlx::query("SELECT state,pause_reason,lease_token,measured_bytes,retained_bytes,reserved_bytes FROM migration_family_refresh_plan WHERE id=$1")
        .bind(prepared.families[0].plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("state"), "paused");
    assert_eq!(row.get::<String, _>("pause_reason"), "storage_limit");
    assert!(row.get::<Option<Uuid>, _>("lease_token").is_none());
    assert_eq!(
        row.get::<i64, _>("measured_bytes"),
        row.get::<i64, _>("retained_bytes")
    );
    assert!(row.get::<i64, _>("reserved_bytes") > 8000);
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap(),
        Progress::Idle
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_worker_authenticates_before_preparation(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let wrong_key = crm_api::config::RawPayloadKey::new([0x42; 32]);
    assert_eq!(
        worker::run_once(&f.pool, &wrong_key, &f.policy)
            .await
            .unwrap(),
        Progress::Paused
    );
    let row = sqlx::query("SELECT state,pause_reason,checkpoint,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1")
        .bind(prepared.families[0].plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(row.get::<String, _>("state"), "paused");
    assert_eq!(
        row.get::<String, _>("pause_reason"),
        "retained_integrity_failed"
    );
    assert_eq!(row.get::<i64, _>("checkpoint"), 0);
    assert_eq!(
        row.get::<i64, _>("measured_bytes"),
        row.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap(),
        Progress::Idle
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_summaries_are_bounded_private_and_workspace_scoped(pool: PgPool) {
    use crm_api::domain::migration::family_refresh::queries::{self, BundlePage};
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepare = || PrepareFamilyRefresh {
        request_id: Uuid::new_v4(),
        parent_import_id: parent,
        core_report_id: None,
        history_capture_id: Some(history),
        families: vec![Family::History],
    };
    let first = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        prepare(),
    )
    .await
    .unwrap();
    // Synthetic terminal fixture; this is not a substitute for the pending
    // typed cancellation command. These equally sized state labels add no bytes.
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='cancelled' WHERE id=$1")
        .bind(first.bundle_id)
        .execute(&pool)
        .await
        .unwrap();
    let second = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        prepare(),
    )
    .await
    .unwrap();
    let page = |cursor| BundlePage {
        parent_import_id: parent,
        limit: Some(1),
        cursor,
    };
    let first_page = queries::list(&f.pool, &f.key, &f.ctx, page(None))
        .await
        .unwrap();
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.items[0].id, second.bundle_id);
    let cursor = first_page.next_cursor.unwrap();
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='cancelled' WHERE id=$1")
        .bind(second.bundle_id)
        .execute(&pool)
        .await
        .unwrap();
    let third = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        prepare(),
    )
    .await
    .unwrap();
    let last_page = queries::list(&f.pool, &f.key, &f.ctx, page(Some(cursor.clone())))
        .await
        .unwrap();
    assert_eq!(last_page.items.len(), 1);
    assert_eq!(last_page.items[0].id, first.bundle_id);
    assert!(
        last_page.next_cursor.is_none(),
        "new bundles are outside the original upper bound"
    );
    assert_eq!(
        queries::list(&f.pool, &f.key, &f.ctx, page(None))
            .await
            .unwrap()
            .items[0]
            .id,
        third.bundle_id
    );
    let detail = queries::detail(&f.pool, &f.ctx, third.bundle_id)
        .await
        .unwrap();
    assert_eq!(detail.bundle.revision, "1");
    assert_eq!(detail.bundle.history_capture_id, Some(history));
    assert!(detail.bundle.digest.is_none());
    assert_eq!(detail.families.len(), 1);
    assert_eq!(detail.families[0].plan_id, third.families[0].plan_id);
    assert_eq!(detail.families[0].counts.units, 0);
    let families = queries::families(&f.pool, &f.ctx, third.bundle_id)
        .await
        .unwrap();
    assert_eq!(families.bundle_revision, "1");
    assert_eq!(
        serde_json::to_value(&families.items).unwrap(),
        serde_json::to_value(&detail.families).unwrap()
    );
    let serialized = serde_json::to_string(&detail).unwrap();
    for private in [
        "nonce",
        "ciphertext",
        "source_nonce",
        "lease_token",
        "IMPORT_BODY_SENTINEL",
        "IMPORT_PHONE_SENTINEL",
    ] {
        assert!(!serialized.contains(private));
    }
    assert!(serialized.len() < 4096);
    assert_eq!(
        serde_json::to_value(&detail).unwrap()["families"][0]["counts"]["units"],
        "0"
    );
    for limit in [0, 51] {
        let mut invalid = page(None);
        invalid.limit = Some(limit);
        assert!(matches!(
            queries::list(&f.pool, &f.key, &f.ctx, invalid).await,
            Err(MigrationError::InvalidInput)
        ));
    }
    let mut wrong_size = page(Some(cursor.clone()));
    wrong_size.limit = Some(2);
    assert!(matches!(
        queries::list(&f.pool, &f.key, &f.ctx, wrong_size).await,
        Err(MigrationError::InvalidInput)
    ));
    let mut tampered = cursor.clone();
    tampered.push('x');
    assert!(matches!(
        queries::list(&f.pool, &f.key, &f.ctx, page(Some(tampered))).await,
        Err(MigrationError::InvalidInput)
    ));
    let actor = crate::common::create_user(
        &pool,
        "summary-reader@example.test",
        "Summary reader",
        "synthetic-summary-pass",
    )
    .await;
    crate::common::add_membership_with(
        &pool,
        f.org,
        actor,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let mut other = f.ctx.clone();
    other.actor_user_id = UserId::new(actor);
    assert!(queries::detail(&f.pool, &other, third.bundle_id)
        .await
        .is_ok());
    assert!(
        matches!(
            queries::list(&f.pool, &f.key, &other, page(Some(cursor.clone()))).await,
            Err(MigrationError::InvalidInput)
        ),
        "another authorized admin cannot reuse this actor's cursor"
    );
    let org = crate::common::create_org(&pool, "Foreign summary workspace").await;
    crate::common::add_membership_with(
        &pool,
        org,
        f.actor,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let mut foreign = f.ctx.clone();
    foreign.organization_id = OrganizationId::new(org);
    assert!(matches!(
        queries::detail(&f.pool, &foreign, third.bundle_id).await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        queries::list(&f.pool, &f.key, &foreign, page(None)).await,
        Err(MigrationError::NotFound)
    ));
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(actor)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        queries::families(&f.pool, &other, third.bundle_id).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        queries::list(&f.pool, &f.key, &other, page(None)).await,
        Err(MigrationError::Forbidden)
    ));
    assert!(matches!(
        queries::detail(&f.pool, &f.ctx, Uuid::new_v4()).await,
        Err(MigrationError::NotFound)
    ));
    sqlx::query("UPDATE organization SET workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matches!(
            queries::list(&f.pool, &f.key, &f.ctx, page(Some(cursor.clone()))).await,
            Err(MigrationError::InvalidInput)
        ),
        "workspace revision invalidates the list cursor"
    );
    // Detached workspace evidence is not readable even to a current admin.
    sqlx::query("DELETE FROM migration_workspace WHERE organization_id=$1")
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        queries::detail(&f.pool, &f.ctx, third.bundle_id).await,
        Err(MigrationError::NotFound)
    ));
    assert!(matches!(
        queries::list(&f.pool, &f.key, &f.ctx, page(Some(cursor))).await,
        Err(MigrationError::NotFound)
    ));
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_mapping_inventory_is_bounded_atomic_and_reuses_shared_sources(
    pool: PgPool,
) {
    use crm_api::domain::migration::{
        family_refresh::{
            cohort, core_source,
            mapping_inventory::{self, Choice, Mapping, Progress as MappingProgress},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(
        Stream::People,
        vec![
            json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3}),
            json!({"id":102,"firstName":"Second","stage":"Lead","assignedUserId":3}),
        ],
    );
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    f.reader.set_records(Stream::CustomFields,vec![json!({"id":10,"name":"customChoice","label":"Choice","type":"dropdown","choices":(0..70).map(|n|format!("Option {n}")).collect::<Vec<_>>()})]);
    let mut invalid_task = db_activity_source::task(22);
    invalid_task["assignedUserId"] = json!("invalid role reference");
    let mut conflicting = db_activity_source::task(21);
    conflicting["personId"] = json!(999);
    let mut outside = db_activity_source::task(30);
    outside["personId"] = json!(999);
    f.reader.set_records(
        Stream::TasksOpen,
        vec![
            db_activity_source::task(21),
            invalid_task,
            conflicting,
            outside,
        ],
    );
    f.reader.set_records(Stream::Notes,vec![json!({"id":11,"personId":101,"createdById":999,"body":"List body must not seed mappings","created":"2026-09-01T12:00:00Z"})]);
    f.reader.set_raw(Stream::NoteDetail,0,200,serde_json::to_vec(&json!({"id":11,"personId":101,"createdById":3,"type":"Note","body":"Retained detail","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null})).unwrap(),false);
    let report=admission::report(&f,parent,vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3,"tags":["Alpha"," alpha "]}),json!({"id":102,"firstName":"Second","stage":"Lead","assignedUserId":3,"tags":["ALPHA"]}),json!({"id":999,"firstName":"Outside cohort","tags":["Excluded tag"]})]).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Metadata, Family::Activity],
        },
    )
    .await
    .unwrap();
    let claim = worker::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(claim.plan, prepared.families[0].plan_id);
    cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
        .await
        .unwrap();
    for step in 0..30 {
        if core_source::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == core_source::Progress::Finished
        {
            break;
        }
        assert!(step < 29);
    }
    assert_mapping_capture_order(&pool, &claim).await;
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('after',mapping_after,'current',mapping_current,'offset',mapping_offset,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    // UUID order may put an excluded Person first; advance that zero-cost row so
    // fault/capacity injection always exercises a real mapping insertion.
    loop {
        let next=sqlx::query("SELECT s.source_person_id,s.kind FROM migration_family_refresh_source s WHERE s.id=crm_family_refresh_next_mapping_source($1,$2,'metadata',(SELECT mapping_after FROM migration_family_refresh_plan WHERE id=$3))").bind(f.org).bind(claim.bundle).bind(claim.plan).fetch_one(&pool).await.unwrap();
        if next.get::<Option<String>, _>("source_person_id").as_deref() != Some("999") {
            break;
        }
        assert_eq!(
            mapping_inventory::run_once(&f.pool, &f.key, &f.policy, &claim)
                .await
                .unwrap(),
            MappingProgress::Advanced
        );
    }
    let checkpoint_before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('after',mapping_after,'current',mapping_current,'offset',mapping_offset,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    assert_eq!(before["retained"], checkpoint_before["retained"]);
    let mut tiny = f.policy.clone();
    tiny.run_ceiling_bytes = 1;
    assert_eq!(
        mapping_inventory::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        MappingProgress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_mapping_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF ROW(NEW.mapping_after,NEW.mapping_current,NEW.mapping_offset) IS DISTINCT FROM ROW(OLD.mapping_after,OLD.mapping_current,OLD.mapping_offset) THEN RAISE EXCEPTION 'synthetic mapping fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_mapping_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_mapping_fault()").execute(&pool).await.unwrap();
    assert!(
        mapping_inventory::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_mapping_fault ON migration_family_refresh_plan; DROP FUNCTION test_mapping_fault()").execute(&pool).await.unwrap();
    let checkpoint_after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('after',mapping_after,'current',mapping_current,'offset',mapping_offset,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(claim.plan).fetch_one(&pool).await.unwrap();
    assert_eq!(checkpoint_before, checkpoint_after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1"
        )
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    for query in ["UPDATE migration_family_refresh_plan SET mappings_complete=true WHERE id=$1","UPDATE migration_family_refresh_plan SET mapping_current=crm_family_refresh_next_mapping_source(organization_id,bundle_id,family,mapping_after),mapping_offset=51 WHERE id=$1"] {
        let mut tx=f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(claim.token.to_string()).execute(&mut *tx).await.unwrap();
        assert!(sqlx::query(query).bind(claim.plan).execute(&mut *tx).await.is_err());
        tx.rollback().await.unwrap();
    }
    let mut partial = false;
    let mut inventory_cursor = None;
    let mut count = 0_i64;
    for step in 0..10 {
        let progress = mapping_inventory::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap();
        let actual: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1",
        )
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            actual - count <= 50,
            "one step discovers at most 50 mappings"
        );
        count = actual;
        let offset: i32 = sqlx::query_scalar(
            "SELECT mapping_offset FROM migration_family_refresh_plan WHERE id=$1",
        )
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap();
        partial |= offset == 50;
        if offset == 50 {
            use crm_api::domain::migration::family_refresh::mapping_queries;
            let page = mapping_queries::mappings(
                &f.pool,
                &f.key,
                &f.ctx,
                claim.bundle,
                mapping_page(claim.plan, None),
            )
            .await
            .unwrap();
            assert!(!page.inventory_complete);
            inventory_cursor = page.next_cursor;
            assert!(inventory_cursor.is_some());
        }
        if progress == MappingProgress::Finished {
            break;
        }
        assert!(step < 9);
    }
    assert!(
        partial,
        "the field's option list crosses the element checkpoint"
    );
    assert_eq!(count, 72, "one field, 70 choices and one folded tag group");
    use crm_api::domain::migration::family_refresh::mapping_queries;
    assert!(
        matches!(
            mapping_queries::mappings(
                &f.pool,
                &f.key,
                &f.ctx,
                claim.bundle,
                mapping_page(claim.plan, inventory_cursor)
            )
            .await,
            Err(MigrationError::InvalidInput)
        ),
        "inventory progress invalidates a partial mapping cursor"
    );
    assert_mapping_reader(&pool, &f, &claim).await;
    let rows = sqlx::query("SELECT * FROM migration_family_refresh_mapping WHERE plan_id=$1")
        .bind(claim.plan)
        .fetch_all(&pool)
        .await
        .unwrap();
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Metadata,
        revision: 1,
    };
    for row in rows {
        let mapping: Mapping = scope
            .open(
                &f.key,
                row.get("id"),
                Purpose::Mapping,
                row.get("nonce"),
                row.get("ciphertext"),
            )
            .unwrap();
        if mapping.kind == "tag" {
            assert_eq!(
                mapping.label.as_deref(),
                Some("Alpha"),
                "first retained Person/item/element chooses the representative spelling"
            );
        }
        assert!(matches!(mapping.choice, Choice::Hold));
        assert_eq!(row.get::<String, _>("disposition"), "hold");
        assert_eq!(mapping.source_key, row.get::<Vec<u8>, _>("source_key_hmac"));
        assert_eq!(
            Some(mapping.source.unwrap().row),
            row.get::<Option<Uuid>, _>("source_row_id")
        );
        if mapping.kind == "field" {
            assert!(
                !mapping.creation_allowed,
                "more than 50 options cannot create a new native field"
            );
        }
        if mapping.kind == "option" {
            assert!(row.get::<Option<Uuid>, _>("parent_id").is_some());
        }
    }
    let retained: i64 =
        sqlx::query_scalar("SELECT retained_bytes FROM migration_family_refresh_plan WHERE id=$1")
            .bind(claim.plan)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        mapping_inventory::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        MappingProgress::Finished
    );
    assert_eq!(
        retained,
        sqlx::query_scalar::<_, i64>(
            "SELECT measured_bytes FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(claim.plan)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    worker::release(&f.pool, &claim).await.unwrap();
    // Admission now enables the 72 bounded catalog outcomes as well as activity.
    for step in 0..300 {
        if worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() == Progress::Idle {
            break;
        }
        assert!(step < 299);
    }
    let activity = prepared.families[1].plan_id;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1 AND NOT qualified").bind(activity).fetch_one(&pool).await.unwrap(),0,"an invalid record does not poison intrinsic validity of a shared role value");

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1"
        )
        .bind(activity)
        .fetch_one(&pool)
        .await
        .unwrap(),
        4,
        "duplicate task actors/kinds share mappings; notes add only their detail author"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_source WHERE plan_id=$1"
        )
        .bind(activity)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "activity reuses the metadata payer's encrypted index"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_mapping m JOIN migration_family_refresh_source s ON s.id=m.source_row_id WHERE m.plan_id=$1 AND s.plan_id=$2").bind(activity).bind(claim.plan).fetch_one(&pool).await.unwrap(),4);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(activity)
        .fetch_one(&pool)
        .await
        .unwrap(),
        4,
        "activity traversal counts each identity once, including held and excluded work"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1 AND kind='catalog'").bind(claim.plan).fetch_one(&pool).await.unwrap(), 72, "each catalog mapping receives its own bounded outcome");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1 AND reason='source_conflict'").bind(activity).fetch_one(&pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1 AND disposition='excluded'").bind(activity).fetch_one(&pool).await.unwrap(),1);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT source_walk_complete FROM migration_family_refresh_plan WHERE id=$1"
    )
    .bind(activity)
    .fetch_one(&pool)
    .await
    .unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_result WHERE bundle_id=$1"
        )
        .bind(claim.bundle)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "preparation cannot execute native work"
    );
}

fn mapping_page(
    plan_id: Uuid,
    cursor: Option<String>,
) -> crm_api::domain::migration::family_refresh::mapping_queries::MappingPage {
    use crm_api::domain::migration::family_refresh::mapping_queries::{
        Disposition, Kind, MappingPage,
    };
    MappingPage {
        family: Family::Metadata,
        plan_id,
        kind: Some(Kind::Option),
        disposition: Some(Disposition::Hold),
        limit: Some(25),
        cursor,
    }
}
async fn assert_mapping_reader(
    pool: &PgPool,
    f: &import_support::Fixture,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
) {
    use crm_api::domain::migration::family_refresh::mapping_queries::{self, Disposition, Kind};
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT to_regclass('family_refresh_mapping_filter') IS NOT NULL"
        )
        .fetch_one(pool)
        .await
        .unwrap(),
        "filtered mapping review migration is installed"
    );
    let first = mapping_queries::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        claim.bundle,
        mapping_page(claim.plan, None),
    )
    .await
    .unwrap();
    assert!(first.inventory_complete);
    assert_eq!(first.plan_revision, "1");
    assert_eq!(first.items.len(), 25);
    let cursor = first.next_cursor.clone().unwrap();
    let encoded = serde_json::to_string(&first).unwrap();
    assert!(encoded.len() < 512 * 1024);
    for private in ["nonce", "ciphertext", "source_key", "source_row_id"] {
        assert!(!encoded.contains(private));
    }
    let mut ids = std::collections::BTreeSet::new();
    for item in &first.items {
        assert!(ids.insert(item.id));
        assert!(item.parent_id.is_some());
        assert!(item.value_qualified);
        assert!(item.creation_allowed, "valid individual options may be created within an existing field even when the source cannot create a whole new field");
    }
    let mut next = first.next_cursor;
    while let Some(cursor) = next {
        let page = mapping_queries::mappings(
            &f.pool,
            &f.key,
            &f.ctx,
            claim.bundle,
            mapping_page(claim.plan, Some(cursor)),
        )
        .await
        .unwrap();
        for item in page.items {
            assert!(ids.insert(item.id), "no repeated mappings across pages");
        }
        next = page.next_cursor;
    }
    assert_eq!(ids.len(), 70);
    for scenario in 0..6 {
        let mut q = mapping_page(claim.plan, Some(cursor.clone()));
        match scenario {
            0 => q.limit = Some(24),
            1 => q.kind = Some(Kind::Tag),
            2 => q.disposition = Some(Disposition::Existing),
            3 => q.cursor = Some(format!("{cursor}x")),
            4 => q.limit = Some(0),
            _ => q.kind = Some(Kind::TaskKind),
        }
        assert!(matches!(
            mapping_queries::mappings(&f.pool, &f.key, &f.ctx, claim.bundle, q).await,
            Err(MigrationError::InvalidInput)
        ));
    }
    let wrong_key = crm_api::config::RawPayloadKey::new([0x63; 32]);
    assert!(matches!(
        mapping_queries::mappings(
            &f.pool,
            &wrong_key,
            &f.ctx,
            claim.bundle,
            mapping_page(claim.plan, None)
        )
        .await,
        Err(MigrationError::Crypto)
    ));
    let mut wrong_plan = mapping_page(Uuid::new_v4(), None);
    wrong_plan.kind = None;
    assert!(matches!(
        mapping_queries::mappings(&f.pool, &f.key, &f.ctx, claim.bundle, wrong_plan).await,
        Err(MigrationError::NotFound)
    ));
    let foreign = crate::common::create_org(pool, "Foreign mapping review").await;
    crate::common::add_membership_with(
        pool,
        foreign,
        f.actor,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    let mut other = f.ctx.clone();
    other.organization_id = OrganizationId::new(foreign);
    assert!(matches!(
        mapping_queries::mappings(
            &f.pool,
            &f.key,
            &other,
            claim.bundle,
            mapping_page(claim.plan, None)
        )
        .await,
        Err(MigrationError::NotFound)
    ));
    let actor = crate::common::create_user(
        pool,
        "mapping-reader@example.test",
        "Mapping reader",
        "synthetic-mapping-pass",
    )
    .await;
    crate::common::add_membership_with(
        pool,
        f.org,
        actor,
        crm_api::domain::admin::Role::Admin,
        crm_api::domain::admin::MembershipStatus::Active,
    )
    .await;
    other = f.ctx.clone();
    other.actor_user_id = UserId::new(actor);
    assert!(mapping_queries::mappings(
        &f.pool,
        &f.key,
        &other,
        claim.bundle,
        mapping_page(claim.plan, None)
    )
    .await
    .is_ok());
    assert!(matches!(
        mapping_queries::mappings(
            &f.pool,
            &f.key,
            &other,
            claim.bundle,
            mapping_page(claim.plan, Some(cursor.clone()))
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(actor)
    .execute(pool)
    .await
    .unwrap();
    assert!(matches!(
        mapping_queries::mappings(
            &f.pool,
            &f.key,
            &other,
            claim.bundle,
            mapping_page(claim.plan, None)
        )
        .await,
        Err(MigrationError::Forbidden)
    ));
    sqlx::query("UPDATE organization SET workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org)
        .execute(pool)
        .await
        .unwrap();
    assert!(matches!(
        mapping_queries::mappings(
            &f.pool,
            &f.key,
            &f.ctx,
            claim.bundle,
            mapping_page(claim.plan, Some(cursor))
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    sqlx::query("UPDATE organization SET workspace_revision=workspace_revision-1 WHERE id=$1")
        .bind(f.org)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_plan_choices_are_versioned_atomic_and_inherited(pool: PgPool) {
    use crm_api::domain::migration::family_refresh::{
        mapping_selection::{MappingPatch, Selection},
        plan_commands::{self, PlanFamilyRefresh},
    };
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    use crm_api::domain::migration::snapshot_source::Stream;
    let mut task = db_activity_source::task(21);
    task["dueDate"] = json!("2026-09-02");
    f.reader.set_records(Stream::TasksOpen, vec![task]);
    f.reader.set_records(Stream::Notes,vec![json!({"id":11,"personId":101,"createdById":3,"body":"List","created":"2026-09-01T12:00:00Z"})]);
    f.reader.set_raw(Stream::NoteDetail,0,200,serde_json::to_vec(&json!({"id":11,"personId":101,"createdById":3,"type":"Note","body":"Detail","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null})).unwrap(),false);
    f.reader.set_records(Stream::CustomFields,vec![json!({"id":10,"name":"customChoice","label":"Choice","type":"dropdown","choices":["First"]})]);
    let native_field = Uuid::new_v4();
    let native_option = Uuid::new_v4();
    sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id) VALUES($1,$2,'Destination','choice',1,$3)").bind(native_field).bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO custom_field_option(id,organization_id,field_id,label,position) VALUES($1,$2,$3,'Native first',1)").bind(native_option).bind(f.org).bind(native_field).execute(&pool).await.unwrap();
    let report=admission::report(&f,parent,vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3,"tags":["New label"]})]).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Metadata, Family::Activity],
        },
    )
    .await
    .unwrap();
    // Isolate read-adapter readiness and manually stepped unit rollback tests.
    // Production admission above has already completed the empty handover.
    sqlx::query("DELETE FROM migration_metadata_catalog_readiness WHERE organization_id=$1")
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    let bundle = prepared.bundle_id;
    let old = prepared.families[0].plan_id;
    let make = |revision: &str, patches: Vec<MappingPatch>| PlanFamilyRefresh {
        request_id: Uuid::new_v4(),
        expected_revision: revision.into(),
        family: Family::Metadata,
        patches,
        source_timezone: None,
    };
    assert!(matches!(
        plan_commands::plan(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            bundle,
            make("1", vec![])
        )
        .await,
        Err(MigrationError::ImportBusy)
    ));
    for step in 0..60 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 59);
    }
    let tag: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='tag'",
    )
    .bind(old)
    .fetch_one(&pool)
    .await
    .unwrap();
    let patch = MappingPatch {
        mapping_id: tag,
        choice: Selection::CreateMatching,
    };
    let request = Uuid::new_v4();
    let field_mapping: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='field'",
    )
    .bind(old)
    .fetch_one(&pool)
    .await
    .unwrap();
    let option_mapping: Uuid = sqlx::query_scalar(
        "SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='option'",
    )
    .bind(old)
    .fetch_one(&pool)
    .await
    .unwrap();
    let input = || PlanFamilyRefresh {
        request_id: request,
        ..make(
            "1",
            vec![
                patch.clone(),
                MappingPatch {
                    mapping_id: option_mapping,
                    choice: Selection::Existing {
                        target_id: native_option,
                    },
                },
                MappingPatch {
                    mapping_id: field_mapping,
                    choice: Selection::Existing {
                        target_id: native_field,
                    },
                },
            ],
        )
    };
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('retained',retained_bytes,'reserved',reserved_bytes) FROM migration_snapshot_storage WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        plan_commands::plan(&f.pool, &f.key, &tiny, &f.ctx, bundle, input()).await,
        Err(MigrationError::StorageLimit)
    ));
    let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('retained',retained_bytes,'reserved',reserved_bytes) FROM migration_snapshot_storage WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_plan WHERE bundle_id=$1"
        )
        .bind(bundle)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    for choice in [
        Selection::Existing {
            target_id: Uuid::new_v4(),
        },
        Selection::Unassigned,
    ] {
        assert!(matches!(
            plan_commands::plan(
                &f.pool,
                &f.key,
                &f.policy,
                &f.ctx,
                bundle,
                make(
                    "1",
                    vec![MappingPatch {
                        mapping_id: tag,
                        choice
                    }]
                )
            )
            .await,
            Err(MigrationError::InvalidImportChoice)
        ));
    }
    assert!(matches!(
        plan_commands::plan(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            bundle,
            make("1", vec![patch.clone(), patch.clone()])
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    sqlx::raw_sql("CREATE FUNCTION test_plan_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic plan fault'; END $$; CREATE TRIGGER test_plan_fault BEFORE INSERT ON migration_family_refresh_mapping_patch FOR EACH ROW EXECUTE FUNCTION test_plan_fault()").execute(&pool).await.unwrap();
    assert!(
        plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, input())
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_plan_fault ON migration_family_refresh_mapping_patch; DROP FUNCTION test_plan_fault()").execute(&pool).await.unwrap();
    let planned = plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, input())
        .await
        .unwrap();
    assert_eq!(planned.revision, "2");
    let successor = planned.families[0].plan_id;
    assert_ne!(old, successor);
    let replay = plan_commands::plan(&f.pool, &f.key, &tiny, &f.ctx, bundle, input())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&planned).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    let mut changed = input();
    changed.patches.clear();
    assert!(matches!(
        plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, changed).await,
        Err(MigrationError::Conflict)
    ));
    assert!(matches!(
        plan_commands::plan(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            bundle,
            make("1", vec![])
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let foreign = crm_api::domain::envelope::CommandContext {
        organization_id: OrganizationId(Uuid::new_v4()),
        ..f.ctx.clone()
    };
    assert!(
        plan_commands::plan(&f.pool, &f.key, &f.policy, &foreign, bundle, input())
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT disposition FROM migration_family_refresh_mapping WHERE id=$1"
        )
        .bind(tag)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "hold"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(old)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "superseded"
    );
    for step in 0..30 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    let target:Uuid=sqlx::query_scalar("SELECT target_id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='tag' AND disposition='create_matching'").bind(successor).fetch_one(&pool).await.unwrap();
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tag WHERE id=$1)")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    assert_metadata_destination_inspection(&pool, &f, bundle, successor).await;
    let third = plan_commands::plan(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        bundle,
        make("2", vec![]),
    )
    .await
    .unwrap();
    for step in 0..30 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    assert_eq!(sqlx::query_scalar::<_,Uuid>("SELECT target_id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='tag'").bind(third.families[0].plan_id).fetch_one(&pool).await.unwrap(),target,"stable prospective ID survives untouched successor revisions");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_source WHERE bundle_id=$1 AND plan_id<>$2").bind(bundle).bind(old).fetch_one(&pool).await.unwrap(),0);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_plan WHERE bundle_id=$1 AND measured_bytes<>retained_bytes").bind(bundle).fetch_one(&pool).await.unwrap(),0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT reserved_bytes FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(successor)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "superseded nonpayer releases its control capacity"
    );
    // Activity choices and timezone have independent revisions over the same index.
    let activity = prepared.families[1].plan_id;
    let roles =
        sqlx::query("SELECT id,kind FROM migration_family_refresh_mapping WHERE plan_id=$1")
            .bind(activity)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(roles.len(), 4);
    let assignee = roles
        .iter()
        .find(|r| r.get::<String, _>("kind") == "task_assignee")
        .unwrap()
        .get::<Uuid, _>("id");
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.member).execute(&pool).await.unwrap();
    let bad = PlanFamilyRefresh {
        family: Family::Activity,
        patches: vec![MappingPatch {
            mapping_id: assignee,
            choice: Selection::Existing {
                target_id: f.member,
            },
        }],
        ..make("3", vec![])
    };
    assert!(matches!(
        plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, bad).await,
        Err(MigrationError::InvalidImportChoice)
    ));
    let patches = roles
        .into_iter()
        .map(|r| MappingPatch {
            mapping_id: r.get("id"),
            choice: if r.get::<String, _>("kind") == "task_kind" {
                Selection::Kind {
                    kind: crm_api::domain::task::TaskKind::FollowUp,
                }
            } else {
                Selection::Existing { target_id: f.actor }
            },
        })
        .collect();
    let activity_cmd = PlanFamilyRefresh {
        request_id: Uuid::new_v4(),
        expected_revision: "3".into(),
        family: Family::Activity,
        patches,
        source_timezone: Some(Some("America/Los_Angeles".into())),
    };
    let fourth = plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, activity_cmd)
        .await
        .unwrap();
    for step in 0..30 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_mapping WHERE plan_id=$1 AND disposition IN ('existing','kind','timezone')").bind(fourth.families[1].plan_id).fetch_one(&pool).await.unwrap(),5);

    assert_activity_mapping_conversion(&pool, &f, bundle, fourth.families[1].plan_id, None).await;
    let carry = PlanFamilyRefresh {
        family: Family::Activity,
        ..make("4", vec![])
    };
    let fifth = plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, carry)
        .await
        .unwrap();
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT disposition FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='timezone'").bind(fifth.families[1].plan_id).fetch_one(&pool).await.unwrap(),"timezone");
    for step in 0..30 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    let clear:PlanFamilyRefresh=serde_json::from_value(json!({"request_id":Uuid::new_v4(),"expected_revision":"5","family":"activity","patches":[],"source_timezone":null})).unwrap();
    let sixth = plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, clear)
        .await
        .unwrap();
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT disposition FROM migration_family_refresh_mapping WHERE plan_id=$1 AND kind='timezone'").bind(sixth.families[1].plan_id).fetch_one(&pool).await.unwrap(),"hold");
    for step in 0..30 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    assert_activity_mapping_conversion(
        &pool,
        &f,
        bundle,
        sixth.families[1].plan_id,
        Some(crm_api::domain::migration::family_refresh::model::Hold::MappingRequired),
    )
    .await;
    let conflict = PlanFamilyRefresh {
        family: Family::Activity,
        source_timezone: Some(Some("UTC".into())),
        ..make("6", vec![])
    };
    let seventh = plan_commands::plan(&f.pool, &f.key, &f.policy, &f.ctx, bundle, conflict)
        .await
        .unwrap();
    for step in 0..30 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    assert_activity_mapping_conversion(
        &pool,
        &f,
        bundle,
        seventh.families[1].plan_id,
        Some(crm_api::domain::migration::family_refresh::model::Hold::SourceConflict),
    )
    .await;
    sqlx::raw_sql(include_str!("fixtures/family_refresh_byte_inventory.sql"))
        .execute(&pool)
        .await
        .unwrap();
}

async fn assert_activity_mapping_conversion(
    pool: &PgPool,
    f: &import_support::Fixture,
    bundle: Uuid,
    plan: Uuid,
    expected_hold: Option<crm_api::domain::migration::family_refresh::model::Hold>,
) {
    use crm_api::domain::migration::family_refresh::{
        activity_delta::Content,
        activity_mapping::{self, Conversion},
        cohort::Claim,
        model::{Hold, Kind},
    };
    // Claim the completed mapping phase without enabling future classification
    // dispatch. This uses the ordinary app lease guards, never a native permit.
    let token = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let epoch:i64=sqlx::query_scalar("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2 RETURNING lease_epoch").bind(plan).bind(f.org).bind(token).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let claim = Claim {
        organization: f.ctx.organization_id,
        bundle,
        plan,
        token,
        epoch,
    };
    let calls = f.reader.calls();
    let note = activity_mapping::convert(&f.pool, &f.key, &claim, Kind::Note, "11")
        .await
        .unwrap();
    let Conversion::Ready(note) = note else {
        panic!("mapped detail must convert")
    };
    assert!(
        matches!(note.evidence.source.content,Content::Note{ref body,author} if body=="Detail" && author==Some(f.actor))
    );
    let task = activity_mapping::convert(&f.pool, &f.key, &claim, Kind::Task, "21")
        .await
        .unwrap();
    if let Some(expected) = expected_hold {
        assert!(
            matches!(task,Conversion::Held(actual) if actual==expected),
            "date-only tasks require a consistent explicit timezone"
        );
    } else {
        let Conversion::Ready(task) = task else {
            panic!("mapped task must convert")
        };
        assert!(
            matches!(task.evidence.source.content,Content::Task{kind:crm_api::domain::task::TaskKind::FollowUp,creator,assignee,due:Some(due),completed:None,..} if creator==Some(f.actor) && assignee==Some(f.actor) && due.to_rfc3339()=="2026-09-03T06:59:59+00:00")
        );
        assert!(task
            .evidence
            .transformations
            .iter()
            .any(|v| v == "date_only_end_of_day_in_confirmed_timezone"));
    }
    assert_eq!(
        f.reader.calls(),
        calls,
        "conversion never contacts the source provider"
    );
    let label: String = sqlx::query_scalar("SELECT display_name FROM app_user WHERE id=$1")
        .bind(f.actor)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE app_user SET display_name='Changed mapping destination' WHERE id=$1")
        .bind(f.actor)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        matches!(
            activity_mapping::convert(&f.pool, &f.key, &claim, Kind::Note, "11")
                .await
                .unwrap(),
            Conversion::Held(Hold::TargetUnavailable)
        ),
        "changed destination snapshots must be reviewed again"
    );
    sqlx::query("UPDATE app_user SET display_name=$2 WHERE id=$1")
        .bind(f.actor)
        .bind(label)
        .execute(pool)
        .await
        .unwrap();
    let foreign = Claim {
        organization: OrganizationId(Uuid::new_v4()),
        ..claim
    };
    assert!(
        activity_mapping::convert(&f.pool, &f.key, &foreign, Kind::Note, "11")
            .await
            .is_err()
    );
    assert!(worker::release(&f.pool, &claim).await.unwrap());
    assert!(
        activity_mapping::convert(&f.pool, &f.key, &claim, Kind::Note, "11")
            .await
            .is_err(),
        "a released lease grants no preparation authority"
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_activity_proposals_are_atomic_stable_and_do_not_write_native(pool: PgPool) {
    activity_execution_scenario(pool, 0).await;
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_activity_execution_is_atomic_and_metered(pool: PgPool) {
    activity_execution_scenario(pool, 1).await;
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_activity_execution_holds_local_changes(pool: PgPool) {
    activity_execution_scenario(pool, 2).await;
}
async fn activity_execution_scenario(pool: PgPool, mode: u8) {
    use crm_api::domain::migration::{
        family_refresh::{
            activity_plan::{self, Decision, Evidence, Prepared},
            cohort::Claim,
            mapping_selection::{MappingPatch, Selection},
            model::{Counts, Kind},
            plan_commands::{self, PlanFamilyRefresh},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    if mode > 0 {
        db_activity_source::note_capture(&book, db_activity_source::note());
    }

    book.set_records(Stream::TasksOpen, vec![db_activity_source::task(21)]);
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (first, ready) = db_activity_source::prepare(&f, parent).await;
    let choices = db_activity_source::choices(&f, first).await;
    let ready = db_activity_source::replan(&f, first, &ready, choices, None).await;
    db_activity_source::confirm(&f, first, &ready).await;
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    if mode > 0 {
        let mut note = db_activity_source::note();
        note["body"] = json!("Updated retained note");
        db_activity_source::note_capture(&f.reader, note);
    }
    let mut changed = db_activity_source::task(21);
    changed["name"] = json!("Updated retained title");
    f.reader.set_records(
        Stream::TasksOpen,
        vec![
            changed,
            db_activity_source::task(22),
            db_activity_source::task(23),
        ],
    );
    let report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let draft = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Activity],
        },
    )
    .await
    .unwrap();
    for step in 0..40 {
        if worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() == Progress::Idle {
            break;
        }
        assert!(step < 39);
    }
    let mappings =
        sqlx::query("SELECT id,kind FROM migration_family_refresh_mapping WHERE plan_id=$1")
            .bind(draft.families[0].plan_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    let patches = mappings
        .into_iter()
        .map(|r| MappingPatch {
            mapping_id: r.get("id"),
            choice: match r.get::<String, _>("kind").as_str() {
                "task_kind" => Selection::Kind {
                    kind: crm_api::domain::task::TaskKind::Call,
                },
                "task_assignee" => Selection::Existing {
                    target_id: f.member,
                },
                _ => Selection::Existing { target_id: f.actor },
            },
        })
        .collect();
    let planned = plan_commands::plan(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        draft.bundle_id,
        PlanFamilyRefresh {
            request_id: Uuid::new_v4(),
            expected_revision: "1".into(),
            family: Family::Activity,
            patches,
            source_timezone: None,
        },
    )
    .await
    .unwrap();
    for step in 0..40 {
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap();
        if sqlx::query_scalar::<_, bool>(
            "SELECT mappings_complete FROM migration_family_refresh_plan WHERE id=$1",
        )
        .bind(planned.families[0].plan_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        {
            break;
        }
        assert!(step < 39);
    }
    let plan = planned.families[0].plan_id;
    let token = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let epoch:i64=sqlx::query_scalar("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2 RETURNING lease_epoch").bind(plan).bind(f.org).bind(token).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let claim = Claim {
        organization: f.ctx.organization_id,
        bundle: draft.bundle_id,
        plan,
        token,
        epoch,
    };
    let cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(claim.bundle).fetch_one(&pool).await.unwrap();
    let checkpoint:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes,'reserved',reserved_bytes,'cursor',checkpoint_id) FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    use crm_api::domain::migration::family_refresh::activity_walk;
    for statement in ["UPDATE migration_family_refresh_plan SET source_walk_complete=true WHERE id=$1","UPDATE migration_family_refresh_plan SET checkpoint_id=(SELECT id FROM migration_family_refresh_source WHERE bundle_id=$2 AND kind IN ('note','task') ORDER BY id DESC LIMIT 1) WHERE id=$1"] {
        let mut tx=f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(token.to_string()).execute(&mut *tx).await.unwrap();
        let query=sqlx::query(statement).bind(plan);
        let result=if statement.contains("$2"){query.bind(claim.bundle).execute(&mut *tx).await}else{query.execute(&mut *tx).await};
        assert!(result.is_err(),"cannot skip source occurrences or declare premature completion");tx.rollback().await.unwrap();
    }
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        activity_plan::prepare_unit(&f.pool, &f.key, &tiny, &claim, cohort, Kind::Task, "21")
            .await
            .unwrap(),
        Prepared::Capacity
    ));
    assert_eq!(
        activity_walk::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        activity_walk::Progress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_activity_plan_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.position<>OLD.position THEN RAISE EXCEPTION 'synthetic activity fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_activity_plan_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_activity_plan_fault()").execute(&pool).await.unwrap();
    assert!(activity_plan::prepare_unit(
        &f.pool,
        &f.key,
        &f.policy,
        &claim,
        cohort,
        Kind::Task,
        "21"
    )
    .await
    .is_err());
    assert!(activity_walk::run_once(&f.pool, &f.key, &f.policy, &claim)
        .await
        .is_err());
    sqlx::raw_sql("DROP TRIGGER test_activity_plan_fault ON migration_family_refresh_plan; DROP FUNCTION test_activity_plan_fault()").execute(&pool).await.unwrap();
    let after_fault:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('position',position,'counts',counts,'retained',retained_bytes,'reserved',reserved_bytes,'cursor',checkpoint_id) FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(checkpoint, after_fault);
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan,
        family: Family::Activity,
        revision: 2,
    };
    let mut ids = Vec::new();
    for source in ["21", "22"] {
        let Prepared::Unit(id) = activity_plan::prepare_unit(
            &f.pool,
            &f.key,
            &f.policy,
            &claim,
            cohort,
            Kind::Task,
            source,
        )
        .await
        .unwrap() else {
            panic!("qualified proposal must persist")
        };
        ids.push(id);
        let row = sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let data: Evidence = scope
            .open(
                &f.key,
                id,
                Purpose::Manifest,
                row.get("nonce"),
                row.get("ciphertext"),
            )
            .unwrap();
        let Decision::Ready {
            baseline, native, ..
        } = data.decision
        else {
            panic!("qualified activity must be ready")
        };
        assert!(native.counts == serde_json::from_value::<Counts>(row.get("counts")).unwrap());
        if source == "21" {
            assert!(baseline.is_some());
            assert_eq!(native.expected_revision, 1);
            assert_eq!(native.counts.updates, 1);
            assert_eq!(native.after["title"], "Updated retained title");
        } else {
            assert!(baseline.is_none());
            assert_eq!(native.expected_revision, 0);
            assert_eq!(native.counts.inserts, 1);
            assert!(!sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM task WHERE id=$1)"
            )
            .bind(native.target)
            .fetch_one(&pool)
            .await
            .unwrap());
        }
    }
    let measured: i64 =
        sqlx::query_scalar("SELECT measured_bytes FROM migration_family_refresh_plan WHERE id=$1")
            .bind(plan)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.member).execute(&pool).await.unwrap();
    for (source, id) in ["21", "22"].into_iter().zip(ids) {
        assert!(
            matches!(activity_plan::prepare_unit(&f.pool,&f.key,&tiny,&claim,cohort,Kind::Task,source).await.unwrap(),Prepared::Unit(actual) if actual==id),
            "replay precedes mutable mapping checks and capacity"
        );
    }
    assert_eq!(
        measured,
        sqlx::query_scalar::<_, i64>(
            "SELECT measured_bytes FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    let Prepared::Unit(held) =
        activity_plan::prepare_unit(&f.pool, &f.key, &f.policy, &claim, cohort, Kind::Task, "23")
            .await
            .unwrap()
    else {
        panic!("mapping hold must persist")
    };
    let row = sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE id=$1")
        .bind(held)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("disposition"), "held");
    assert!(row.get::<Option<Uuid>, _>("target_id").is_none());
    assert_eq!(row.get::<String, _>("reason"), "target_unavailable");
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_head WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let p=sqlx::query("SELECT counts,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    let counts: Counts = serde_json::from_value(p.get("counts")).unwrap();
    assert_eq!(
        (counts.units, counts.updates, counts.inserts, counts.held),
        (3, 1, 1, 1)
    );
    assert_eq!(
        p.get::<i64, _>("measured_bytes"),
        p.get::<i64, _>("retained_bytes")
    );
    worker::release(&f.pool, &claim).await.unwrap();
    for step in 0..30 {
        if worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() == Progress::Idle {
            break;
        }
        assert!(step < 29);
    }
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT source_walk_complete FROM migration_family_refresh_plan WHERE id=$1"
    )
    .bind(plan)
    .fetch_one(&pool)
    .await
    .unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        if mode == 0 { 3 } else { 4 },
        "traversal reuses frozen outcomes"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "ready",
        "worker seals only after source, ownership, proof and count walks complete"
    );
    if mode == 0 {
        return;
    }
    use crm_api::domain::migration::family_refresh::{
        confirmation::{self, ConfirmFamilyRefresh, SelectedPlan},
        execution,
    };
    sqlx::query("UPDATE organization_membership SET status='active' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.member).execute(&pool).await.unwrap();
    let sealed=sqlx::query("SELECT b.revision AS bundle_revision,encode(b.digest,'hex') AS bundle_digest,p.revision,encode(p.digest,'hex') AS digest,p.counts FROM migration_family_refresh_bundle b JOIN migration_family_refresh_plan p ON p.bundle_id=b.id AND p.organization_id=b.organization_id WHERE p.id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    let release = crm_api::auth::workspace::ReleaseReadiness::for_tests();
    confirmation::confirm(
        &f.pool,
        &f.key,
        &release,
        &f.ctx,
        claim.bundle,
        ConfirmFamilyRefresh {
            request_id: Uuid::new_v4(),
            expected_revision: sealed.get::<i64, _>("bundle_revision").to_string(),
            bundle_digest: sealed.get("bundle_digest"),
            families: vec![SelectedPlan {
                family: Family::Activity,
                plan_id: plan,
                plan_revision: sealed.get::<i64, _>("revision").to_string(),
                plan_digest: sealed.get("digest"),
                expected_counts: serde_json::from_value(sealed.get("counts")).unwrap(),
            }],
            acknowledged_exclusions: true,
        },
    )
    .await
    .unwrap();
    if mode == 2 {
        sqlx::query("UPDATE task SET title='Local task title' WHERE organization_id=$1 AND source_external_id LIKE '%:21'").bind(f.org).execute(&pool).await.unwrap();
    }
    let executing = execution::claim_next(&f.pool).await.unwrap().unwrap();
    let checkpoint_sql="SELECT jsonb_build_object('tasks',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=p.organization_id),'heads',(SELECT count(*) FROM migration_family_refresh_head WHERE organization_id=p.organization_id),'identities',(SELECT count(*) FROM migration_activity_identity WHERE organization_id=p.organization_id),'results',(SELECT count(*) FROM migration_family_refresh_result WHERE plan_id=p.id),'position',apply_position,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan p WHERE id=$1";
    let before: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        execution::apply_once(&f.pool, &f.key, &tiny, &release, &executing)
            .await
            .unwrap(),
        execution::Progress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_activity_execute_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic execution fault'; END $$; CREATE TRIGGER test_activity_execute_fault BEFORE INSERT ON migration_family_refresh_result FOR EACH ROW EXECUTE FUNCTION test_activity_execute_fault()").execute(&pool).await.unwrap();
    assert!(
        execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_activity_execute_fault ON migration_family_refresh_result; DROP FUNCTION test_activity_execute_fault()").execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, serde_json::Value>(checkpoint_sql)
            .bind(plan)
            .fetch_one(&pool)
            .await
            .unwrap(),
        before
    );
    // Advance the update/hold, then fault the addition after its native row and
    // exclusive identity have been written. Both must roll back together.
    assert_eq!(
        execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
            .await
            .unwrap(),
        execution::Progress::Advanced
    );
    let before_addition: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION test_activity_execute_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic addition fault'; END $$; CREATE TRIGGER test_activity_execute_fault BEFORE INSERT ON migration_family_refresh_result FOR EACH ROW EXECUTE FUNCTION test_activity_execute_fault()").execute(&pool).await.unwrap();
    assert!(
        execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_activity_execute_fault ON migration_family_refresh_result; DROP FUNCTION test_activity_execute_fault()").execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, serde_json::Value>(checkpoint_sql)
            .bind(plan)
            .fetch_one(&pool)
            .await
            .unwrap(),
        before_addition
    );
    for turn in 0..10 {
        match execution::apply_once(&f.pool, &f.key, &f.policy, &release, &executing)
            .await
            .unwrap()
        {
            execution::Progress::Advanced => assert!(turn < 9),
            execution::Progress::Finished => break,
            execution::Progress::Capacity => panic!("unexpected capacity"),
        }
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT title FROM task WHERE organization_id=$1 AND source_external_id LIKE '%:21'"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        if mode == 2 {
            "Local task title"
        } else {
            "Updated retained title"
        }
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_activity_identity WHERE refresh_plan_id=$1"
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_head WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        if mode == 2 { 2 } else { 3 }
    );
    let results=sqlx::query("SELECT r.*,s.source_id FROM migration_family_refresh_result r JOIN migration_family_refresh_manifest u ON u.id=r.manifest_id AND u.organization_id=r.organization_id JOIN migration_family_refresh_source s ON s.id=u.source_row_id AND s.organization_id=u.organization_id WHERE r.plan_id=$1").bind(plan).fetch_all(&pool).await.unwrap();
    for row in results {
        let source: String = row.get("source_id");
        let held = source == "23" || (mode == 2 && source == "21");
        assert_eq!(
            row.get::<String, _>("disposition"),
            if held { "held" } else { "applied" }
        );
        let data: crm_api::domain::migration::family_refresh::native_baseline::ResultData = scope
            .open(
                &f.key,
                row.get("id"),
                Purpose::Result,
                row.get("nonce"),
                row.get("ciphertext"),
            )
            .unwrap();
        assert_eq!(data.after_state.is_some(), !held);
    }
    let accounting = sqlx::query(
        "SELECT measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1",
    )
    .bind(plan)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        accounting.get::<i64, _>("measured_bytes"),
        accounting.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT body FROM note WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Subject: Source subject\n\nUpdated retained note"
    );
    // A subsequent bundle must authenticate the new refresh first owner, and
    // its ownership walk must retain that identity even without a source row.
    let next_report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let next = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &release,
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(next_report),
            history_capture_id: None,
            families: vec![Family::Activity],
        },
    )
    .await
    .unwrap();
    for step in 0..50 {
        if advance_to_review_phase(&f, "mappings_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 49);
    }
    let next_claim = worker::claim_next(&f.pool).await.unwrap().unwrap();
    let next_cohort:Uuid=sqlx::query_scalar("SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND source_person_id='101'").bind(next.bundle_id).fetch_one(&pool).await.unwrap();
    use crm_api::domain::migration::family_refresh::activity_baseline;
    let baseline = match activity_baseline::discover(
        &f.pool,
        &f.key,
        &next_claim,
        next_cohort,
        Kind::Task,
        "22",
    )
    .await
    .unwrap()
    {
        activity_baseline::Discovery::Proven(b) => b,
        activity_baseline::Discovery::Held(h) => panic!("refresh-owned addition baseline: {h:?}"),
    };
    assert!(baseline.owned);
    assert_eq!(baseline.revision, 1);
    assert_eq!(Some(baseline.result_id), baseline.head_id);
    let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM crm_family_refresh_owned_activity($1,$2) WHERE kind='task' AND source_id='22' AND target_id=$3)").bind(f.org).bind(next.bundle_id).bind(baseline.target_id).fetch_one(&f.pool).await.unwrap();
    assert!(
        owned,
        "refresh first owners remain in subsequent missing-source walks"
    );
    worker::release(&f.pool, &next_claim).await.unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_missing_activity_holds_absence_and_preserves_owned_person_scope(
    pool: PgPool,
) {
    use crm_api::domain::migration::{
        family_refresh::{
            activity_missing::{self, MissingProposal},
            activity_walk::Progress as WalkProgress,
            cohort::Claim,
            model::{Counts, Hold},
        },
        snapshot_source::Stream,
    };
    let book = db_activity_source::book();
    book.set_records(
        Stream::TasksOpen,
        vec![db_activity_source::task(21), db_activity_source::task(22)],
    );
    let f = import_support::fixture_with_book(&pool, book).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let (first, ready) = db_activity_source::prepare(&f, parent).await;
    let choices = db_activity_source::choices(&f, first).await;
    let ready = db_activity_source::replan(&f, first, &ready, choices, None).await;
    db_activity_source::confirm(&f, first, &ready).await;
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut moved = db_activity_source::task(21);
    moved["personId"] = json!(999);
    let mut outside = db_activity_source::task(30);
    outside["personId"] = json!(999);
    f.reader
        .set_records(Stream::TasksOpen, vec![moved, outside]);
    let report = admission::report(
        &f,
        parent,
        vec![
            json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3}),
            json!({"id":999,"firstName":"Outside"}),
        ],
    )
    .await;
    let draft = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Activity],
        },
    )
    .await
    .unwrap();
    let plan = draft.families[0].plan_id;
    for step in 0..40 {
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap();
        if sqlx::query_scalar::<_, bool>(
            "SELECT source_walk_complete FROM migration_family_refresh_plan WHERE id=$1",
        )
        .bind(plan)
        .fetch_one(&pool)
        .await
        .unwrap()
        {
            break;
        }
        assert!(step < 39);
    }
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT m.reason FROM migration_family_refresh_manifest m JOIN migration_family_refresh_source s ON s.id=m.source_row_id WHERE m.plan_id=$1 AND s.source_id='21'").bind(plan).fetch_one(&pool).await.unwrap(),"identity_mismatch","a moved owned identity must not become an ordinary exclusion");
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT m.disposition FROM migration_family_refresh_manifest m JOIN migration_family_refresh_source s ON s.id=m.source_row_id WHERE m.plan_id=$1 AND s.source_id='30'").bind(plan).fetch_one(&pool).await.unwrap(),"excluded");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM crm_family_refresh_owned_activity($1,$2)"
        )
        .bind(Uuid::new_v4())
        .bind(draft.bundle_id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    let token = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let epoch:i64=sqlx::query_scalar("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2 RETURNING lease_epoch").bind(plan).bind(f.org).bind(token).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let claim = Claim {
        organization: f.ctx.organization_id,
        bundle: draft.bundle_id,
        plan,
        token,
        epoch,
    };
    for statement in ["UPDATE migration_family_refresh_plan SET owned_walk_complete=true WHERE id=$1","UPDATE migration_family_refresh_plan SET owned_activity_kind='task',owned_activity_source_id='22' WHERE id=$1","UPDATE migration_family_refresh_plan SET owned_after=gen_random_uuid() WHERE id=$1"] {
        let mut tx=f.pool.begin().await.unwrap();sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(token.to_string()).execute(&mut *tx).await.unwrap();
        assert!(sqlx::query(statement).bind(plan).execute(&mut *tx).await.is_err());tx.rollback().await.unwrap();
    }
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert_eq!(
        activity_missing::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        WalkProgress::Capacity,
        "an observed unit's variable cursor still requires admission"
    );
    assert_eq!(
        activity_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        WalkProgress::Advanced
    );
    let checkpoint:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('kind',owned_activity_kind,'source',owned_activity_source_id,'count',counts,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(checkpoint["source"], "21");
    assert_eq!(
        activity_missing::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        WalkProgress::Capacity
    );
    sqlx::raw_sql("CREATE FUNCTION test_missing_activity_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.owned_activity_source_id IS DISTINCT FROM OLD.owned_activity_source_id THEN RAISE EXCEPTION 'synthetic missing activity fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_missing_activity_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_missing_activity_fault()").execute(&pool).await.unwrap();
    assert!(
        activity_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_missing_activity_fault ON migration_family_refresh_plan; DROP FUNCTION test_missing_activity_fault()").execute(&pool).await.unwrap();
    let after_fault:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('kind',owned_activity_kind,'source',owned_activity_source_id,'count',counts,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes) FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    assert_eq!(checkpoint, after_fault);
    assert_eq!(
        activity_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        WalkProgress::Advanced
    );
    let row = sqlx::query(
        "SELECT * FROM migration_family_refresh_manifest WHERE plan_id=$1 AND source_id='22'",
    )
    .bind(plan)
    .fetch_one(&pool)
    .await
    .unwrap();
    let scope = Scope {
        organization: f.ctx.organization_id,
        bundle: claim.bundle,
        plan,
        family: Family::Activity,
        revision: 1,
    };
    let data: MissingProposal = scope
        .open(
            &f.key,
            row.get("id"),
            Purpose::Manifest,
            row.get("nonce"),
            row.get("ciphertext"),
        )
        .unwrap();
    assert!(data.source_not_observed);
    assert_eq!(data.source_id, "22");
    assert_eq!(data.reason, Hold::SourceNotObserved);
    assert!(data.baseline.is_some());
    assert!(row.get::<Option<Uuid>, _>("target_id").is_none());
    assert!(row.get::<Option<Uuid>, _>("source_row_id").is_none());
    assert_eq!(
        activity_missing::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap(),
        WalkProgress::Finished
    );
    assert_eq!(
        activity_missing::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        WalkProgress::Finished
    );
    let p=sqlx::query("SELECT counts,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
    let counts: Counts = serde_json::from_value(p.get("counts")).unwrap();
    assert_eq!((counts.units, counts.held, counts.excluded), (3, 2, 1));
    assert_eq!(
        p.get::<i64, _>("measured_bytes"),
        p.get::<i64, _>("retained_bytes")
    );
    assert_eq!(
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM task t WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_head WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    sqlx::raw_sql(include_str!("fixtures/family_refresh_byte_inventory.sql"))
        .execute(&pool)
        .await
        .unwrap();
    worker::release(&f.pool, &claim).await.unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_metadata_catalog_qualifies_complete_names_and_choices(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            cohort, core_source, mapping_inventory,
            metadata_catalog::{self, Qualification},
            model::Hold,
        },
        snapshot_source::Stream,
    };
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    f.reader.set_records(Stream::CustomFields,vec![
        json!({"id":10,"name":"customUnique","label":"Unique","type":"dropdown","choices":(0..70).map(|n|format!("Option {n}")).collect::<Vec<_>>()}),
        json!({"id":11,"name":"customDuplicate","label":"First","type":"text"}),
        json!({"id":12,"name":"customDuplicate","label":"Second","type":"text"}),
        json!({"id":13,"name":"customLocale","label":"Locale","type":"dropdown","choices":["İ","i"]}),
        json!({"id":14,"name":"customVariant","label":"Before","type":"text"}),
        json!({"id":14,"name":"customVariant","label":"After","type":"text"}),
        json!({"id":15,"name":"customduplicate","label":"Exact case differs","type":"text"}),
        json!({"id":16,"name":"customOther","label":"Variant one","type":"text"}),
        json!({"id":16,"name":"customHidden","label":"Variant two","type":"text"}),
        json!({"id":17,"name":"customHidden","label":"Must see second occurrence","type":"text"}),
    ]);
    let report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let draft = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Metadata],
        },
    )
    .await
    .unwrap();
    let claim = worker::claim_next(&f.pool).await.unwrap().unwrap();
    assert_eq!(claim.plan, draft.families[0].plan_id);
    cohort::freeze_page(&f.pool, &claim, &f.policy, 50)
        .await
        .unwrap();
    for step in 0..30 {
        if core_source::index_page(&f.pool, &f.key, &claim, &f.policy)
            .await
            .unwrap()
            == core_source::Progress::Finished
        {
            break;
        }
        assert!(step < 29);
    }
    assert!(matches!(
        metadata_catalog::qualify_field(&f.pool, &f.key, &claim, "10").await,
        Err(MigrationError::ImportBusy)
    ));
    for step in 0..30 {
        if mapping_inventory::run_once(&f.pool, &f.key, &f.policy, &claim)
            .await
            .unwrap()
            == mapping_inventory::Progress::Finished
        {
            break;
        }
        assert!(step < 29);
    }
    for id in ["10", "15"] {
        assert!(matches!(metadata_catalog::qualify_field(&f.pool,&f.key,&claim,id).await.unwrap(),Qualification::Ready(_)),"valid definitions remain eligible independent of create limits and exact-case-distinct names");
    }
    for id in ["11", "12", "14", "16", "17"] {
        assert!(matches!(
            metadata_catalog::qualify_field(&f.pool, &f.key, &claim, id)
                .await
                .unwrap(),
            Qualification::Held(Hold::SourceConflict)
        ));
    }
    let folded: bool = sqlx::query_scalar("SELECT lower('İ')=lower('i')")
        .fetch_one(&pool)
        .await
        .unwrap();
    let locale = metadata_catalog::qualify_field(&f.pool, &f.key, &claim, "13")
        .await
        .unwrap();
    assert!(
        matches!(locale, Qualification::Held(Hold::SourceConflict)) == folded,
        "source choices follow the native database collation"
    );
    assert!(matches!(
        metadata_catalog::qualify_field(&f.pool, &f.key, &claim, "99")
            .await
            .unwrap(),
        Qualification::Held(Hold::SourceNotObserved)
    ));
    let foreign = cohort::Claim {
        organization: OrganizationId(Uuid::new_v4()),
        ..claim
    };
    assert!(
        metadata_catalog::qualify_field(&f.pool, &f.key, &foreign, "10")
            .await
            .is_err()
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_source WHERE plan_id=$1 AND field_name_hmac IS NOT NULL").bind(claim.plan).fetch_one(&pool).await.unwrap(),10);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM custom_field WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "qualification creates no native catalog objects"
    );
    // Simulate a retained pre-017 index without rewriting source ciphertext.
    let mut legacy = pool.begin().await.unwrap();
    sqlx::raw_sql("ALTER TABLE migration_family_refresh_source DISABLE TRIGGER USER")
        .execute(&mut *legacy)
        .await
        .unwrap();
    sqlx::query("UPDATE migration_family_refresh_source SET catalog_names_indexed=false WHERE bundle_id=$1 AND kind='field'").bind(claim.bundle).execute(&mut *legacy).await.unwrap();
    sqlx::raw_sql("ALTER TABLE migration_family_refresh_source ENABLE TRIGGER USER")
        .execute(&mut *legacy)
        .await
        .unwrap();
    legacy.commit().await.unwrap();
    assert!(matches!(
        metadata_catalog::qualify_field(&f.pool, &f.key, &claim, "10")
            .await
            .unwrap(),
        Qualification::Held(Hold::SourceUnavailable)
    ));
    sqlx::raw_sql(include_str!("fixtures/family_refresh_byte_inventory.sql"))
        .execute(&pool)
        .await
        .unwrap();
}

async fn assert_metadata_destination_inspection(
    pool: &PgPool,
    f: &import_support::Fixture,
    bundle: Uuid,
    plan: Uuid,
) {
    use crm_api::domain::migration::family_refresh::{
        cohort::Claim,
        metadata_destination::{self, Inspection},
        model::Hold,
    };
    let token = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let epoch:i64=sqlx::query_scalar("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2 RETURNING lease_epoch").bind(plan).bind(f.org).bind(token).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let claim = Claim {
        organization: f.ctx.organization_id,
        bundle,
        plan,
        token,
        epoch,
    };
    let rows=sqlx::query("SELECT id,kind,target_id FROM migration_family_refresh_mapping WHERE plan_id=$1 ORDER BY kind").bind(plan).fetch_all(pool).await.unwrap();
    let first: Uuid = rows[0].get("id");
    assert!(matches!(
        metadata_destination::inspect(&f.pool, &f.key, &claim, first).await,
        Err(MigrationError::ReleaseNotReady)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_metadata_identity WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(pool)
        .await
        .unwrap(),
        0
    );
    // Restore the fixture registry after proving that reads never activate it.
    sqlx::query("INSERT INTO migration_metadata_catalog_readiness(organization_id,state,activated_at,activated_by_user_id,engine_version) VALUES($1,'ready',clock_timestamp(),$2,'fub-admitted-metadata-v1')").bind(f.org).bind(f.actor).execute(pool).await.unwrap();
    use crm_api::domain::migration::family_refresh::{
        catalog_plan::{self, Prepared},
        model::Counts,
    };
    let option_first = rows
        .iter()
        .find(|r| r.get::<String, _>("kind") == "option")
        .unwrap()
        .get::<Uuid, _>("id");
    assert!(
        matches!(
            catalog_plan::prepare_unit(&f.pool, &f.key, &f.policy, &claim, option_first).await,
            Err(MigrationError::ImportBusy)
        ),
        "parent catalog outcome precedes an option outcome"
    );
    let checkpoint_sql="SELECT jsonb_build_object('position',position,'counts',counts,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes,'cursor',catalog_after,'catalog_complete',catalog_walk_complete) FROM migration_family_refresh_plan WHERE id=$1";
    let before: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap();
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        catalog_plan::prepare_unit(&f.pool, &f.key, &tiny, &claim, first)
            .await
            .unwrap(),
        Prepared::Capacity
    ));
    sqlx::raw_sql("CREATE FUNCTION test_catalog_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.position<>OLD.position THEN RAISE EXCEPTION 'synthetic catalog checkpoint fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_catalog_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_catalog_fault()").execute(pool).await.unwrap();
    assert!(
        catalog_plan::prepare_unit(&f.pool, &f.key, &f.policy, &claim, first)
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER test_catalog_fault ON migration_family_refresh_plan; DROP FUNCTION test_catalog_fault()").execute(pool).await.unwrap();
    let after: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        after, before,
        "failed catalog checkpoint and capacity admission roll back manifests/counts/bytes"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap(),
        0
    );
    let mut units = std::collections::BTreeMap::new();
    let mut tag = Uuid::nil();
    let mut field = Uuid::nil();
    let mut option = Uuid::nil();
    let mut native_field = Uuid::nil();
    let calls = f.reader.calls();
    for row in rows {
        let id: Uuid = row.get("id");
        let Inspection::Ready(evidence) =
            metadata_destination::inspect(&f.pool, &f.key, &claim, id)
                .await
                .unwrap()
        else {
            panic!("explicit valid catalog choice must qualify");
        };
        assert_eq!(evidence.target, row.get::<Uuid, _>("target_id"));
        let Inspection::Ready(replay) = metadata_destination::inspect(&f.pool, &f.key, &claim, id)
            .await
            .unwrap()
        else {
            panic!("unchanged catalog choice must remain ready");
        };
        assert_eq!(
            serde_json::to_value(&evidence).unwrap(),
            serde_json::to_value(&replay).unwrap()
        );
        let Prepared::Unit(unit) =
            catalog_plan::prepare_unit(&f.pool, &f.key, &f.policy, &claim, id)
                .await
                .unwrap()
        else {
            panic!("catalog unit must fit");
        };
        assert!(
            matches!(catalog_plan::prepare_unit(&f.pool,&f.key,&tiny,&claim,id).await.unwrap(),Prepared::Unit(replayed) if replayed==unit)
        );
        units.insert(id, unit);
        match row.get::<String, _>("kind").as_str() {
            "tag" => tag = id,
            "field" => {
                field = id;
                native_field = evidence.target;
            }
            "option" => option = id,
            _ => panic!("unexpected kind"),
        }
    }
    let totals: serde_json::Value =
        sqlx::query_scalar("SELECT counts FROM migration_family_refresh_plan WHERE id=$1")
            .bind(plan)
            .fetch_one(pool)
            .await
            .unwrap();
    let totals: Counts = serde_json::from_value(totals).unwrap();
    assert_eq!(
        (
            totals.units,
            totals.inserts,
            totals.already_current,
            totals.held
        ),
        (3, 1, 2, 0)
    );
    assert!(totals.reconciles());
    let retained: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(retained["measured"], retained["retained"]);
    assert_eq!(
        f.reader.calls(),
        calls,
        "catalog validation never contacts FUB"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tag WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(pool)
            .await
            .unwrap(),
        0,
        "prospective tag stays absent"
    );
    let mut forged = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(token.to_string()).execute(&mut *forged).await.unwrap();
    let error=sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,source_row_id,position,kind,source_key_hmac,target_id,disposition,counts,nonce,ciphertext,added_byte_bound) SELECT $1,u.bundle_id,u.plan_id,u.organization_id,u.source_row_id,p.position+1,'catalog',decode(repeat('ab',32),'hex'),u.target_id,u.disposition,u.counts,u.nonce,u.ciphertext,u.added_byte_bound FROM migration_family_refresh_manifest u JOIN migration_family_refresh_plan p ON p.id=u.plan_id WHERE u.id=$2")
        .bind(Uuid::new_v4()).bind(units[&field]).execute(&mut *forged).await.unwrap_err();
    assert!(
        error
            .as_database_error()
            .is_some_and(|e| e.message().contains("invalid family catalog unit binding")),
        "application SQL cannot omit the typed catalog mapping owner"
    );
    forged.rollback().await.unwrap();
    use crm_api::domain::migration::family_refresh::catalog_walk::{
        self, Progress as CatalogProgress,
    };
    for statement in ["UPDATE migration_family_refresh_plan SET catalog_walk_complete=true WHERE id=$1", "UPDATE migration_family_refresh_plan SET catalog_after=(SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 ORDER BY source_sequence DESC,source_ordinal DESC,source_row_id DESC,source_element DESC,id DESC LIMIT 1) WHERE id=$1"] {
        let mut forbidden=f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.family_refresh_lease',$1,true)").bind(token.to_string()).execute(&mut *forbidden).await.unwrap();
        assert!(sqlx::query(statement).bind(plan).execute(&mut *forbidden).await.is_err(),"cannot skip catalog outcomes or prematurely complete the walk");
        forbidden.rollback().await.unwrap();
    }
    let before_walk: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION test_catalog_walk_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.catalog_after IS DISTINCT FROM OLD.catalog_after THEN RAISE EXCEPTION 'synthetic catalog walk fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_catalog_walk_fault BEFORE UPDATE ON migration_family_refresh_plan FOR EACH ROW EXECUTE FUNCTION test_catalog_walk_fault()").execute(pool).await.unwrap();
    assert!(catalog_walk::run_once(&f.pool, &f.key, &tiny, &claim)
        .await
        .is_err());
    sqlx::raw_sql("DROP TRIGGER test_catalog_walk_fault ON migration_family_refresh_plan; DROP FUNCTION test_catalog_walk_fault()").execute(pool).await.unwrap();
    let failed_walk: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(failed_walk, before_walk);
    for _ in 0..3 {
        assert_eq!(
            catalog_walk::run_once(&f.pool, &f.key, &tiny, &claim)
                .await
                .unwrap(),
            CatalogProgress::Advanced
        );
    }
    assert_eq!(
        catalog_walk::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        CatalogProgress::Finished
    );
    assert_eq!(
        catalog_walk::run_once(&f.pool, &f.key, &tiny, &claim)
            .await
            .unwrap(),
        CatalogProgress::Finished
    );
    let completed: serde_json::Value = sqlx::query_scalar(checkpoint_sql)
        .bind(plan)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(completed["catalog_complete"], true);
    for key in ["position", "counts", "measured", "retained", "reserved"] {
        assert_eq!(completed[key],before_walk[key],"replaying units into a fixed-width checkpoint neither duplicates outcomes nor charges bytes");
    }
    assert!(sqlx::query_scalar::<_,bool>("SELECT state='preparing' AND digest IS NULL AND NOT source_walk_complete AND NOT owned_walk_complete FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(pool).await.unwrap(),"catalog exhaustion is not complete Person preparation or readiness");
    let original:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('label',label,'updated_at',updated_at) FROM custom_field WHERE id=$1").bind(native_field).fetch_one(pool).await.unwrap();
    sqlx::query("UPDATE custom_field SET label='Changed destination' WHERE id=$1")
        .bind(native_field)
        .execute(pool)
        .await
        .unwrap();
    assert!(
        matches!(catalog_plan::prepare_unit(&f.pool,&f.key,&tiny,&claim,field).await.unwrap(),Prepared::Unit(unit) if Some(&unit)==units.get(&field)),
        "immutable preview replay precedes mutable destination checks"
    );
    for id in [field, option] {
        assert!(
            matches!(
                metadata_destination::inspect(&f.pool, &f.key, &claim, id)
                    .await
                    .unwrap(),
                Inspection::Held(Hold::TargetUnavailable)
            ),
            "a changed field also holds dependent options"
        );
    }
    sqlx::query("UPDATE custom_field SET label=$2,updated_at=($3::text)::timestamptz WHERE id=$1")
        .bind(native_field)
        .bind(original["label"].as_str().unwrap())
        .bind(original["updated_at"].as_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    let conflicting = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tag(id,organization_id,name,created_by_user_id) VALUES($1,$2,'NEW LABEL',$3)",
    )
    .bind(conflicting)
    .bind(f.org)
    .bind(f.actor)
    .execute(pool)
    .await
    .unwrap();
    assert!(
        matches!(
            metadata_destination::inspect(&f.pool, &f.key, &claim, tag)
                .await
                .unwrap(),
            Inspection::Held(Hold::TargetUnavailable)
        ),
        "native database label collision holds create matching"
    );
    sqlx::query("DELETE FROM tag WHERE id=$1")
        .bind(conflicting)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tag(organization_id,name,created_by_user_id) SELECT $1,'Capacity '||n,$2 FROM generate_series(1,200) n").bind(f.org).bind(f.actor).execute(pool).await.unwrap();
    assert!(
        matches!(
            metadata_destination::inspect(&f.pool, &f.key, &claim, tag)
                .await
                .unwrap(),
            Inspection::Held(Hold::TargetUnavailable)
        ),
        "full native tag capacity holds creation"
    );
    sqlx::query("DELETE FROM tag WHERE organization_id=$1 AND name LIKE 'Capacity %'")
        .bind(f.org)
        .execute(pool)
        .await
        .unwrap();
    for capture_order in [false, true] {
        let mut compatibility = pool.begin().await.unwrap();
        sqlx::raw_sql("ALTER TABLE migration_family_refresh_bundle DISABLE TRIGGER USER")
            .execute(&mut *compatibility)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE migration_family_refresh_bundle SET mapping_capture_order=$2 WHERE id=$1",
        )
        .bind(bundle)
        .bind(capture_order)
        .execute(&mut *compatibility)
        .await
        .unwrap();
        sqlx::raw_sql("ALTER TABLE migration_family_refresh_bundle ENABLE TRIGGER USER")
            .execute(&mut *compatibility)
            .await
            .unwrap();
        compatibility.commit().await.unwrap();
        let outcome = metadata_destination::inspect(&f.pool, &f.key, &claim, tag)
            .await
            .unwrap();
        if capture_order {
            assert!(matches!(outcome, Inspection::Ready(_)));
        } else {
            assert!(
                matches!(outcome, Inspection::Held(Hold::SourceUnavailable)),
                "old tag representatives cannot silently gain execution eligibility"
            );
        }
    }
    let foreign = Claim {
        organization: OrganizationId(Uuid::new_v4()),
        ..claim
    };
    assert!(
        metadata_destination::inspect(&f.pool, &f.key, &foreign, tag)
            .await
            .is_err()
    );
    assert!(worker::release(&f.pool, &claim).await.unwrap());
    assert!(metadata_destination::inspect(&f.pool, &f.key, &claim, tag)
        .await
        .is_err());
}

pub(crate) async fn claim_metadata_inspection(
    f: &import_support::Fixture,
    bundle: Uuid,
    plan: Uuid,
) -> crm_api::domain::migration::family_refresh::cohort::Claim {
    let token = Uuid::new_v4();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let epoch:i64=sqlx::query_scalar("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=lease_epoch+1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2 RETURNING lease_epoch").bind(plan).bind(f.org).bind(token).fetch_one(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    crm_api::domain::migration::family_refresh::cohort::Claim {
        organization: f.ctx.organization_id,
        bundle,
        plan,
        token,
        epoch,
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_catalog_creation_requires_all_options_and_distinct_targets(pool: PgPool) {
    use crm_api::domain::migration::{
        family_refresh::{
            mapping_selection::{MappingPatch, Selection},
            metadata_destination::{self, Inspection},
            model::Hold,
            plan_commands::{self, PlanFamilyRefresh},
        },
        snapshot_source::Stream,
    };
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    f.reader.set_records(Stream::CustomFields,vec![
        json!({"id":10,"name":"customChoice","label":"Complete choice","type":"dropdown","choices":["First","Second"]}),
        json!({"id":11,"name":"customLeft","label":"Same label","type":"text"}),
        json!({"id":12,"name":"customRight","label":"SAME LABEL","type":"text"}),
    ]);
    let report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let draft = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Metadata],
        },
    )
    .await
    .unwrap();
    for step in 0..40 {
        if advance_to_review_phase(&f, "owned_walk_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 39);
    }
    assert!(sqlx::query_scalar::<_,bool>("SELECT state='ready' FROM migration_metadata_catalog_readiness WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap(), "typed refresh admission completed the handover");
    let mut current = draft;
    for round in 0..2 {
        let rows=sqlx::query("SELECT m.id,m.kind,m.source_element FROM migration_family_refresh_mapping m WHERE m.plan_id=$1 ORDER BY m.id").bind(current.families[0].plan_id).fetch_all(&pool).await.unwrap();
        let patches = rows
            .iter()
            .filter(|r| {
                round == 1
                    || r.get::<String, _>("kind") != "option"
                    || r.get::<i32, _>("source_element") == 1
            })
            .map(|r| MappingPatch {
                mapping_id: r.get("id"),
                choice: Selection::CreateMatching,
            })
            .collect();
        current = plan_commands::plan(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            current.bundle_id,
            PlanFamilyRefresh {
                request_id: Uuid::new_v4(),
                expected_revision: current.revision.clone(),
                family: Family::Metadata,
                patches,
                source_timezone: None,
            },
        )
        .await
        .unwrap();
        for step in 0..40 {
            if advance_to_review_phase(&f, "owned_walk_complete").await == Progress::Idle {
                break;
            }
            assert!(step < 39);
        }
        assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT catalog_walk_complete FROM migration_family_refresh_plan WHERE id=$1"
            )
            .bind(current.families[0].plan_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            "ready shared registry permits bounded worker catalog preparation"
        );
        let claim =
            claim_metadata_inspection(&f, current.bundle_id, current.families[0].plan_id).await;
        let rows=sqlx::query("SELECT m.id,m.kind,m.target_id,s.source_id FROM migration_family_refresh_mapping m JOIN migration_family_refresh_source s ON s.id=m.source_row_id AND s.organization_id=m.organization_id WHERE m.plan_id=$1 ORDER BY m.kind,m.id").bind(claim.plan).fetch_all(&pool).await.unwrap();
        for row in rows {
            let prepared = crm_api::domain::migration::family_refresh::catalog_plan::prepare_unit(
                &f.pool,
                &f.key,
                &f.policy,
                &claim,
                row.get("id"),
            )
            .await
            .unwrap();
            assert!(matches!(
                prepared,
                crm_api::domain::migration::family_refresh::catalog_plan::Prepared::Unit(_)
            ));
            let inspected = metadata_destination::inspect(&f.pool, &f.key, &claim, row.get("id"))
                .await
                .unwrap();
            if row.get::<String, _>("source_id") == "10" && round == 1 {
                let Inspection::Ready(evidence) = inspected else {
                    panic!("complete explicit field and options should qualify independently of another label collision");
                };
                assert_eq!(evidence.target, row.get::<Uuid, _>("target_id"));
                assert_eq!(
                    evidence.field.is_some(),
                    row.get::<String, _>("kind") == "option"
                );
            } else {
                assert!(
                    matches!(inspected, Inspection::Held(Hold::TargetUnavailable)),
                    "partial choice creation and colliding field labels remain held"
                );
            }
        }
        let counts: serde_json::Value =
            sqlx::query_scalar("SELECT counts FROM migration_family_refresh_plan WHERE id=$1")
                .bind(claim.plan)
                .fetch_one(&pool)
                .await
                .unwrap();
        let counts: crm_api::domain::migration::family_refresh::model::Counts =
            serde_json::from_value(counts).unwrap();
        assert_eq!(counts.units, 6);
        assert_eq!(counts.inserts, if round == 1 { 3 } else { 0 });
        assert_eq!(counts.held, if round == 1 { 3 } else { 6 });
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1 AND kind='metadata' AND disposition='held'").bind(claim.plan).fetch_one(&pool).await.unwrap(),1,"catalog prerequisites cannot supply missing Person first coverage");
        assert!(counts.reconciles());
        worker::release(&f.pool, &claim).await.unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM custom_field WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let native = Uuid::new_v4();
    sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id) VALUES($1,$2,'Explicit existing','text',1,$3)").bind(native).bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    let ids:Vec<Uuid>=sqlx::query_scalar("SELECT m.id FROM migration_family_refresh_mapping m JOIN migration_family_refresh_source s ON s.id=m.source_row_id AND s.organization_id=m.organization_id WHERE m.plan_id=$1 AND m.kind='field' AND s.source_id IN ('11','12')").bind(current.families[0].plan_id).fetch_all(&pool).await.unwrap();
    current = plan_commands::plan(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        current.bundle_id,
        PlanFamilyRefresh {
            request_id: Uuid::new_v4(),
            expected_revision: current.revision.clone(),
            family: Family::Metadata,
            patches: ids
                .into_iter()
                .map(|id| MappingPatch {
                    mapping_id: id,
                    choice: Selection::Existing { target_id: native },
                })
                .collect(),
            source_timezone: None,
        },
    )
    .await
    .unwrap();
    for step in 0..40 {
        if advance_to_review_phase(&f, "owned_walk_complete").await == Progress::Idle {
            break;
        }
        assert!(step < 39);
    }
    let claim = claim_metadata_inspection(&f, current.bundle_id, current.families[0].plan_id).await;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM migration_family_refresh_mapping WHERE plan_id=$1 AND target_id=$2",
    )
    .bind(claim.plan)
    .bind(native)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ids.len(), 2);
    for id in ids {
        assert!(matches!(
            metadata_destination::inspect(&f.pool, &f.key, &claim, id)
                .await
                .unwrap(),
            Inspection::Held(Hold::SourceConflict)
        ));
    }
    worker::release(&f.pool, &claim).await.unwrap();
}

async fn assert_mapping_capture_order(
    pool: &PgPool,
    claim: &crm_api::domain::migration::family_refresh::cohort::Claim,
) {
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT mapping_capture_order FROM migration_family_refresh_bundle WHERE id=$1"
    )
    .bind(claim.bundle)
    .fetch_one(pool)
    .await
    .unwrap());
    for capture_order in [false, true] {
        let mut compatibility = pool.begin().await.unwrap();
        sqlx::raw_sql("ALTER TABLE migration_family_refresh_bundle DISABLE TRIGGER USER")
            .execute(&mut *compatibility)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE migration_family_refresh_bundle SET mapping_capture_order=$2 WHERE id=$1",
        )
        .bind(claim.bundle)
        .bind(capture_order)
        .execute(&mut *compatibility)
        .await
        .unwrap();
        sqlx::raw_sql("ALTER TABLE migration_family_refresh_bundle ENABLE TRIGGER USER")
            .execute(&mut *compatibility)
            .await
            .unwrap();
        compatibility.commit().await.unwrap();
        let order = if capture_order {
            "capture_sequence,ordinal,id"
        } else {
            "id"
        };
        for family in ["metadata", "activity"] {
            let expected:Vec<Uuid>=sqlx::query_scalar(&format!("SELECT id FROM migration_family_refresh_source WHERE bundle_id=$1 AND organization_id=$2 AND (($3='metadata' AND kind IN ('person','field')) OR ($3='activity' AND kind IN ('note','task'))) ORDER BY {order}")).bind(claim.bundle).bind(claim.organization.0).bind(family).fetch_all(pool).await.unwrap();
            let mut after = None;
            for id in expected {
                let next: Option<Uuid> = sqlx::query_scalar(
                    "SELECT crm_family_refresh_next_mapping_source($1,$2,$3,$4)",
                )
                .bind(claim.organization.0)
                .bind(claim.bundle)
                .bind(family)
                .bind(after)
                .fetch_one(pool)
                .await
                .unwrap();
                assert_eq!(next, Some(id));
                after = next;
            }
            assert!(sqlx::query_scalar::<_, Option<Uuid>>(
                "SELECT crm_family_refresh_next_mapping_source($1,$2,$3,$4)"
            )
            .bind(claim.organization.0)
            .bind(claim.bundle)
            .bind(family)
            .bind(after)
            .fetch_one(pool)
            .await
            .unwrap()
            .is_none());
        }
    }
}

// Stop at the exact preparation boundary a test intends to inspect. The real
// worker now continues beyond those boundaries to seal a ready plan.
pub(crate) async fn advance_to_review_phase(
    f: &import_support::Fixture,
    boundary: &str,
) -> Progress {
    assert!(matches!(
        boundary,
        "mappings_complete" | "catalog_walk_complete" | "owned_walk_complete"
    ));
    let query=format!("SELECT NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE organization_id=$1 AND state='preparing' AND NOT {boundary})");
    if sqlx::query_scalar::<_, bool>(&query)
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap()
    {
        return Progress::Idle;
    }
    worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap()
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_revoked_executor_is_durably_paused(pool: PgPool) {
    let (f, parent, history, _) = history::fixture(&pool).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: None,
            history_capture_id: Some(history),
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    let claim = worker::claim_next(&f.pool).await.unwrap().unwrap();
    sqlx::query(
        "UPDATE organization_membership SET role='member' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap(),
        Progress::Paused
    );
    let p=sqlx::query("SELECT state,pause_reason,lease_token,measured_bytes,retained_bytes FROM migration_family_refresh_plan WHERE id=$1").bind(prepared.families[0].plan_id).fetch_one(&pool).await.unwrap();
    assert_eq!(p.get::<String, _>("state"), "paused");
    assert_eq!(p.get::<String, _>("pause_reason"), "executor_revoked");
    assert!(p.get::<Option<Uuid>, _>("lease_token").is_none());
    assert_eq!(
        p.get::<i64, _>("measured_bytes"),
        p.get::<i64, _>("retained_bytes")
    );
    assert!(!worker::release(&f.pool, &claim).await.unwrap());
    assert_eq!(
        worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap(),
        Progress::Idle
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_result WHERE bundle_id=$1"
        )
        .bind(prepared.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    use crm_api::domain::migration::family_refresh::{
        lifecycle::{self, FamilyControl},
        resume,
    };
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.member)
    .execute(&pool)
    .await
    .unwrap();
    let mut admin = f.ctx.clone();
    admin.actor_user_id = UserId::new(f.member);
    let request = Uuid::new_v4();
    let command = || FamilyControl {
        request_id: request,
        expected_revision: prepared.revision.clone(),
        families: vec![Family::History],
    };
    let release = crm_api::auth::workspace::ReleaseReadiness::for_tests();
    let mut tiny = f.policy.clone();
    tiny.org_ceiling_bytes = 1;
    assert!(matches!(
        resume::resume(
            &f.pool,
            &f.key,
            &tiny,
            &release,
            &admin,
            prepared.bundle_id,
            command()
        )
        .await,
        Err(MigrationError::StorageLimit)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT executor_user_id FROM migration_family_refresh_bundle WHERE id=$1"
        )
        .bind(prepared.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        f.actor
    );
    let resumed = resume::resume(
        &f.pool,
        &f.key,
        &f.policy,
        &release,
        &admin,
        prepared.bundle_id,
        command(),
    )
    .await
    .unwrap();
    assert_eq!(resumed.state, "preparing");
    let replay = resume::resume(
        &f.pool,
        &f.key,
        &tiny,
        &release,
        &admin,
        prepared.bundle_id,
        command(),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&resumed).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT executor_user_id FROM migration_family_refresh_bundle WHERE id=$1"
        )
        .bind(prepared.bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        f.member
    );
    let cancelled = lifecycle::cancel(
        &f.pool,
        &f.key,
        &admin,
        prepared.bundle_id,
        FamilyControl {
            request_id: Uuid::new_v4(),
            expected_revision: resumed.revision,
            families: vec![Family::History],
        },
    )
    .await
    .unwrap();
    assert_eq!(cancelled.state, "cancelled");
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_partial_cancel_keeps_shared_payer_work(pool: PgPool) {
    use crm_api::domain::migration::family_refresh::lifecycle::{self, FamilyControl};
    let (f, parent, _, _) = history::fixture(&pool).await;
    let report = admission::report(
        &f,
        parent,
        vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &crm_api::auth::workspace::ReleaseReadiness::for_tests(),
        &f.ctx,
        PrepareFamilyRefresh {
            request_id: Uuid::new_v4(),
            parent_import_id: parent,
            core_report_id: Some(report),
            history_capture_id: None,
            families: vec![Family::Metadata, Family::Activity],
        },
    )
    .await
    .unwrap();
    let cancelled = lifecycle::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        prepared.bundle_id,
        FamilyControl {
            request_id: Uuid::new_v4(),
            expected_revision: prepared.revision,
            families: vec![Family::Metadata],
        },
    )
    .await
    .unwrap();
    assert_eq!(cancelled.state, "preparing");
    assert!(sqlx::query_scalar::<_,bool>("SELECT cancel_requested AND state='preparing' FROM migration_family_refresh_plan WHERE id=$1").bind(prepared.families[0].plan_id).fetch_one(&pool).await.unwrap());
    for turn in 0..100 {
        if worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() == Progress::Idle {
            break;
        }
        assert!(turn < 99);
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(prepared.families[0].plan_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "cancelled"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM migration_family_refresh_plan WHERE id=$1"
        )
        .bind(prepared.families[1].plan_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "ready"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE plan_id=$1"
        )
        .bind(prepared.families[0].plan_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "cancelled payer never classifies its native units"
    );
    sqlx::raw_sql(include_str!("fixtures/family_refresh_byte_inventory.sql"))
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn family_refresh_history_execution_is_atomic_versioned_and_refresh_owned(pool: PgPool) {
    use crm_api::{
        auth::workspace::ReleaseReadiness,
        domain::migration::{
            family_refresh::{
                confirmation::{self, ConfirmFamilyRefresh, SelectedPlan},
                execution,
            },
            history_capture_source::Stream,
        },
    };
    let (f, parent, initial, book) = history::fixture(&pool).await;
    let import = history::ready(&f, parent, initial).await;
    history::confirm(&f, import).await;
    history::drain(&f).await;
    let original:serde_json::Value=sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=$1").bind(f.org).fetch_one(&pool).await.unwrap();
    book.set_records(Stream::Events,vec![json!({"id":1,"personId":101,"type":"Changed","created":"unknown","description":"PRIVATE_CORRECTION"}),json!({"id":4,"personId":101,"type":"New retained event","created":"2026-01-05T00:00:00Z","description":"PRIVATE_ADDITION"})]);
    book.set_records(Stream::TextMessages,vec![json!({"id":3,"personId":102,"userId":3,"created":"2026-01-06T00:00:00Z","message":"PRIVATE_TEXT_CORRECTION","status":"Updated vendor status"})]);
    let release = ReleaseReadiness::for_tests();
    for round in 0..2 {
        if round == 1 {
            book.set_records(Stream::Events,vec![json!({"id":1,"personId":101,"type":"Changed","created":"unknown","description":"PRIVATE_CORRECTION"}),json!({"id":4,"personId":101,"type":"New retained event corrected","created":"unknown","description":"PRIVATE_ADDITION_CORRECTED"})]);
        }
        let (capture_id, _) = capture::propose(&f, parent).await;
        capture::confirm(&f, capture_id).await;
        capture::drain(&f, &book).await;
        let bundle = commands::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &release,
            &f.ctx,
            PrepareFamilyRefresh {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
                core_report_id: None,
                history_capture_id: Some(capture_id),
                families: vec![Family::History],
            },
        )
        .await
        .unwrap();
        let plan = bundle.families[0].plan_id;
        for n in 0..200 {
            let state: String =
                sqlx::query_scalar("SELECT state FROM migration_family_refresh_bundle WHERE id=$1")
                    .bind(bundle.bundle_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            if state == "ready" {
                break;
            }
            assert_ne!(state, "paused");
            assert!(n < 199);
            worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap();
        }
        let sealed=sqlx::query("SELECT b.revision AS bundle_revision,encode(b.digest,'hex') AS bundle_digest,p.revision,encode(p.digest,'hex') AS digest,p.counts FROM migration_family_refresh_bundle b JOIN migration_family_refresh_plan p ON p.bundle_id=b.id AND p.organization_id=b.organization_id WHERE p.id=$1").bind(plan).fetch_one(&pool).await.unwrap();
        let counts: crm_api::domain::migration::family_refresh::model::Counts =
            serde_json::from_value(sealed.get("counts")).unwrap();
        assert_eq!(counts.units, 4);
        assert_eq!(
            counts.held,
            0,
            "{}",
            sealed.get::<serde_json::Value, _>("counts")
        );
        assert_eq!(counts.inserts, u64::from(round == 0));
        assert_eq!(counts.history_corrections, if round == 0 { 2 } else { 1 });
        let request = Uuid::new_v4();
        let input = || ConfirmFamilyRefresh {
            request_id: request,
            expected_revision: sealed.get::<i64, _>("bundle_revision").to_string(),
            bundle_digest: sealed.get("bundle_digest"),
            families: vec![SelectedPlan {
                family: Family::History,
                plan_id: plan,
                plan_revision: sealed.get::<i64, _>("revision").to_string(),
                plan_digest: sealed.get("digest"),
                expected_counts: serde_json::from_value(sealed.get("counts")).unwrap(),
            }],
            acknowledged_exclusions: true,
        };
        let confirmed =
            confirmation::confirm(&f.pool, &f.key, &release, &f.ctx, bundle.bundle_id, input())
                .await
                .unwrap();
        let replay = confirmation::confirm_with_readiness(
            &f.pool,
            &f.key,
            None,
            &f.ctx,
            bundle.bundle_id,
            input(),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(confirmed).unwrap(),
            serde_json::to_value(replay).unwrap()
        );
        let claim = execution::claim_next(&f.pool).await.unwrap().unwrap();
        let checkpoint="SELECT jsonb_build_object('identities',(SELECT jsonb_agg(to_jsonb(i) ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=p.organization_id),'heads',(SELECT jsonb_agg(to_jsonb(h) ORDER BY identity_id) FROM migration_family_refresh_history_head h WHERE organization_id=p.organization_id),'results',(SELECT count(*) FROM migration_family_refresh_result WHERE plan_id=p.id),'initial_displays',(SELECT count(*) FROM migration_history_import_display WHERE organization_id=p.organization_id),'correction_displays',(SELECT count(*) FROM migration_family_refresh_history_display WHERE organization_id=p.organization_id),'read_state',(SELECT jsonb_agg(to_jsonb(s) ORDER BY person_id) FROM migration_history_review_state s WHERE organization_id=p.organization_id),'position',apply_position,'measured',measured_bytes,'retained',retained_bytes,'reserved',reserved_bytes,'org',(SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=p.organization_id)) FROM migration_family_refresh_plan p WHERE id=$1";
        for position in 1..=4 {
            let unit=sqlx::query("SELECT kind,disposition FROM migration_family_refresh_manifest WHERE plan_id=$1 AND position=$2").bind(plan).bind(position as i64).fetch_one(&pool).await.unwrap();
            let before: serde_json::Value = sqlx::query_scalar(checkpoint)
                .bind(plan)
                .fetch_one(&pool)
                .await
                .unwrap();
            let mut tiny = f.policy.clone();
            tiny.org_ceiling_bytes = 1;
            assert_eq!(
                execution::apply_once(&f.pool, &f.key, &tiny, &release, &claim)
                    .await
                    .unwrap(),
                execution::Progress::Capacity
            );
            let disposition: String = unit.get("disposition");
            if disposition != "already_current" {
                let kind: String = unit.get("kind");
                let suffix = if disposition == "insert" {
                    "imported"
                } else {
                    "corrected"
                };
                let table = format!("fub_{kind}_record_{suffix}");
                sqlx::raw_sql(&format!("CREATE FUNCTION test_history_execute_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic history execution fault'; END $$; CREATE TRIGGER zz_test_history_execute_fault AFTER INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION test_history_execute_fault()")).execute(&pool).await.unwrap();
                assert!(
                    execution::apply_once(&f.pool, &f.key, &f.policy, &release, &claim)
                        .await
                        .is_err()
                );
                sqlx::raw_sql(&format!("DROP TRIGGER zz_test_history_execute_fault ON {table}; DROP FUNCTION test_history_execute_fault()")).execute(&pool).await.unwrap();
            }
            assert_eq!(
                sqlx::query_scalar::<_, serde_json::Value>(checkpoint)
                    .bind(plan)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                before
            );
            assert_eq!(
                execution::apply_once(&f.pool, &f.key, &f.policy, &release, &claim)
                    .await
                    .unwrap(),
                execution::Progress::Advanced
            );
        }
        assert_eq!(
            execution::apply_once(&f.pool, &f.key, &f.policy, &release, &claim)
                .await
                .unwrap(),
            execution::Progress::Finished
        );
        let result=sqlx::query("SELECT state,counts,results,measured_bytes,retained_bytes,reserved_bytes FROM migration_family_refresh_plan WHERE id=$1").bind(plan).fetch_one(&pool).await.unwrap();
        assert_eq!(result.get::<String, _>("state"), "completed");
        assert_eq!(
            result.get::<serde_json::Value, _>("counts"),
            result.get::<serde_json::Value, _>("results")
        );
        assert_eq!(
            result.get::<i64, _>("measured_bytes"),
            result.get::<i64, _>("retained_bytes")
        );
        assert_eq!(result.get::<i64, _>("reserved_bytes"), 0);
    }
    assert_eq!(sqlx::query_scalar::<_,serde_json::Value>("SELECT jsonb_agg(to_jsonb(i)-ARRAY['refresh_bundle_id','refresh_plan_id','refresh_manifest_id'] ORDER BY id) FROM migration_history_import_identity i WHERE organization_id=$1 AND refresh_bundle_id IS NULL").bind(f.org).fetch_one(&pool).await.unwrap(),serde_json::Value::Array(original.as_array().unwrap().iter().cloned().map(|mut v|{for key in ["refresh_bundle_id","refresh_plan_id","refresh_manifest_id"]{v.as_object_mut().unwrap().remove(key);}v}).collect()));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_history_import_identity WHERE organization_id=$1 AND refresh_bundle_id IS NOT NULL").bind(f.org).fetch_one(&pool).await.unwrap(),1);
    let head=sqlx::query("SELECT h.*,i.refresh_manifest_id FROM migration_family_refresh_history_head h JOIN migration_history_import_identity i ON i.id=h.identity_id AND i.organization_id=h.organization_id WHERE h.organization_id=$1 AND i.refresh_bundle_id IS NOT NULL").bind(f.org).fetch_one(&pool).await.unwrap();
    assert_eq!(head.get::<i64, _>("version"), 2);
    assert!(head
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("source_created_at")
        .is_none());
    let auth = crm_api::auth::AuthContext {
        actor_user_id: UserId::new(f.actor),
        actor_email: "review@synthetic.test".into(),
        actor_display_name: "Review admin".into(),
        active_organization_id: OrganizationId::new(f.org),
        active_organization_name: "Synthetic".into(),
        role: crm_api::domain::admin::Role::Admin,
    };
    let person = crm_api::ids::PersonId::new(head.get("person_id"));
    let identity: Uuid = head.get("identity_id");
    let versions = crm_api::domain::migration::history_review::versions(
        &f.pool,
        &f.key,
        &auth,
        person,
        "fub_event_record_imported",
        identity,
        &Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.items.len(), 2);
    assert_eq!(versions.items[0]["version"], "2");
    assert_eq!(versions.items[1]["version"], "1");
    assert!(!serde_json::to_string(&versions)
        .unwrap()
        .contains("PRIVATE"));
    let bytes:i64=sqlx::query_scalar("SELECT (SELECT octet_length(nonce)+octet_length(ciphertext) FROM migration_history_import_display WHERE id=$1)::bigint+(SELECT sum(octet_length(nonce)+octet_length(ciphertext)) FROM migration_family_refresh_history_display WHERE organization_id=$2 AND identity_id=$3)").bind(head.get::<Uuid,_>("refresh_manifest_id")).bind(f.org).bind(identity).fetch_one(&pool).await.unwrap();
    let before: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut erase = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.family_refresh_reader','fub-family-refresh-v1',true),set_config('crm.history_reader','fub-history-timeline-v1',true),set_config('crm.admitted_history_reader','fub-admitted-history-v1',true)").execute(&mut *erase).await.unwrap();
    sqlx::query("UPDATE migration_history_import_identity SET erased_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(identity).bind(f.org).execute(&mut *erase).await.unwrap();
    erase.commit().await.unwrap();
    let after: i64 = sqlx::query_scalar(
        "SELECT retained_bytes FROM migration_snapshot_storage WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before - after, bytes);
    assert!(crm_api::domain::migration::history_review::versions(
        &f.pool,
        &f.key,
        &auth,
        person,
        "fub_event_record_imported",
        identity,
        &Default::default()
    )
    .await
    .is_err());
}

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
        commands::prepare(&f.pool, &f.key, &tiny, &f.ctx, input()).await,
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
    let prepared = commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, input())
        .await
        .unwrap();
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
    let replay = commands::prepare(&f.pool, &f.key, &tiny, &f.ctx, input())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(replay).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
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
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, changed).await,
        Err(MigrationError::Conflict)
    ));
    let mut second = input();
    second.request_id = Uuid::new_v4();
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, second).await,
        Err(MigrationError::Conflict)
    ));
    let mut stranger = f.ctx.clone();
    stranger.actor_user_id = UserId::new(Uuid::new_v4());
    assert!(matches!(
        commands::prepare(&f.pool, &f.key, &f.policy, &stranger, input()).await,
        Err(MigrationError::Forbidden)
    ));
    stranger = f.ctx.clone();
    stranger.organization_id = OrganizationId::new(Uuid::new_v4());
    assert!(
        commands::prepare(&f.pool, &f.key, &f.policy, &stranger, input())
            .await
            .is_err()
    );
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
        assert_eq!(
            plan.get::<String, _>("phase"),
            if history { "classify" } else { "mappings" }
        );
        assert_eq!(plan.get::<bool, _>("source_walk_complete"), history);
        assert_eq!(plan.get::<bool, _>("owned_walk_complete"), history);
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
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, input()).await,
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
        commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, foreign).await,
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
            commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, cmd).await,
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
    let first = commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, prepare())
        .await
        .unwrap();
    // Synthetic terminal fixture; this is not a substitute for the pending
    // typed cancellation command. These equally sized state labels add no bytes.
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='cancelled' WHERE id=$1")
        .bind(first.bundle_id)
        .execute(&pool)
        .await
        .unwrap();
    let second = commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, prepare())
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
    let third = commands::prepare(&f.pool, &f.key, &f.policy, &f.ctx, prepare())
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
    let f = import_support::fixture_with_book(&pool, db_activity_source::book()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    f.reader.set_records(Stream::CustomFields,vec![json!({"id":10,"name":"customChoice","label":"Choice","type":"dropdown","choices":(0..70).map(|n|format!("Option {n}")).collect::<Vec<_>>()})]);
    let mut invalid_task = db_activity_source::task(22);
    invalid_task["assignedUserId"] = json!("invalid role reference");
    f.reader.set_records(
        Stream::TasksOpen,
        vec![db_activity_source::task(21), invalid_task],
    );
    f.reader.set_records(Stream::Notes,vec![json!({"id":11,"personId":101,"createdById":999,"body":"List body must not seed mappings","created":"2026-09-01T12:00:00Z"})]);
    f.reader.set_raw(Stream::NoteDetail,0,200,serde_json::to_vec(&json!({"id":11,"personId":101,"createdById":3,"type":"Note","body":"Retained detail","isHtml":false,"created":"2026-09-01T12:00:00Z","updated":null})).unwrap(),false);
    let report=admission::report(&f,parent,vec![json!({"id":101,"firstName":"Synthetic","stage":"Lead","assignedUserId":3,"tags":["Alpha"," alpha "]}),json!({"id":999,"firstName":"Outside cohort","tags":["Excluded tag"]})]).await;
    let prepared = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
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
    for step in 0..30 {
        if worker::run_once(&f.pool, &f.key, &f.policy).await.unwrap() == Progress::Idle {
            break;
        }
        assert!(step < 29);
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
            "SELECT count(*) FROM migration_family_refresh_manifest WHERE bundle_id=$1"
        )
        .bind(claim.bundle)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "mapping discovery grants no native action"
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
        assert!(!item.creation_allowed);
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

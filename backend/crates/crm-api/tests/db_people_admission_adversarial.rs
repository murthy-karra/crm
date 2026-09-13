//! D-078 adversarial acceptance through retained snapshots, sealed plans, and
//! the real admission worker.  These tests mutate only a migrator-owned
//! synthetic fixture after capture; ordinary commands still use crm_app.
use crate::{db_activity_source, db_people_admission_execution, import_support};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        core_change_reports, people_admission, people_admission_worker,
        snapshot::{self, SnapshotRequest, SourceAction},
        snapshot_source::Stream,
        MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn number(value: &Value) -> i64 {
    value.as_str().unwrap().parse().unwrap()
}

fn confirmation(detail: &Value, request_id: Uuid) -> people_admission::ConfirmPeopleAdmission {
    let plan = &detail["plan"];
    people_admission::ConfirmPeopleAdmission {
        request_id,
        plan_id: Uuid::parse_str(plan["id"].as_str().unwrap()).unwrap(),
        plan_revision: number(&plan["revision"]),
        plan_digest: plan["digest"].as_str().unwrap().to_owned(),
        eligible_count: number(&plan["counts"]["eligible"]),
        acknowledged_coverage: true,
        acknowledged_mappings: true,
        acknowledged_distinct_contacts: true,
        acknowledged_review_hold: true,
    }
}

async fn prepare_and_pause_after_source_fault(
    f: &import_support::Fixture,
    report_id: Uuid,
) -> Uuid {
    let prepared = people_admission::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        people_admission::PreparePeopleAdmission {
            request_id: Uuid::new_v4(),
            report_id,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let id = Uuid::parse_str(prepared["admission_id"].as_str().unwrap()).unwrap();
    let mut failure = None;
    for _ in 0..24 {
        match people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        {
            Ok(true) => continue,
            Ok(false) => panic!("corrupt retained source unexpectedly drained"),
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    assert!(
        failure.as_ref().is_some_and(|error| matches!(
            error,
            MigrationError::SourceNotEligible | MigrationError::Crypto
        )),
        "retained source fault must fail closed: {failure:?}"
    );
    id
}

async fn seal_after_repreview(f: &import_support::Fixture, id: Uuid) -> Value {
    for _ in 0..24 {
        let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap();
        if detail["state"] == "ready" {
            return detail;
        }
        assert!(people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap());
    }
    panic!("repreview did not seal within bounded fixture work")
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn corrupt_retained_capture_ordinal_hmac_or_original_baseline_fails_closed(migrator: PgPool) {
    for fault in ["capture", "ordinal", "semantic_hmac", "baseline"] {
        let f = import_support::fixture(&migrator, import_support::default_people()).await;
        let parent = db_activity_source::completed_parent(&f).await;
        let report = db_people_admission_execution::report(
            &f,
            parent,
            vec![json!({"id":106,"firstName":"Adversarial source","stage":"Lead","assignedUserId":3})],
        )
        .await;
        let newer: Uuid = sqlx::query_scalar(
            "SELECT newer_snapshot_id FROM migration_core_change_report WHERE id=$1",
        )
        .bind(report)
        .fetch_one(&migrator)
        .await
        .unwrap();
        match fault {
            "capture" => {
                let capture: Uuid = sqlx::query_scalar("SELECT id FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream='people' ORDER BY sequence DESC LIMIT 1")
                    .bind(newer).bind(f.org).fetch_one(&migrator).await.unwrap();
                sqlx::query("UPDATE migration_snapshot_capture SET accepted=false,classification='no_progress' WHERE id=$1")
                    .bind(capture).execute(&migrator).await.unwrap();
            }
            "ordinal" => {
                sqlx::query("UPDATE migration_snapshot_record SET ordinal=99 WHERE snapshot_id=$1 AND organization_id=$2 AND family='people' AND source_id='106'")
                    .bind(newer).bind(f.org).execute(&migrator).await.unwrap();
            }
            "semantic_hmac" => {
                sqlx::query("UPDATE migration_snapshot_record SET semantic_hmac=decode(repeat('00',32),'hex') WHERE snapshot_id=$1 AND organization_id=$2 AND family='people' AND source_id='106'")
                    .bind(newer).bind(f.org).execute(&migrator).await.unwrap();
            }
            "baseline" => {
                let baseline: Uuid = sqlx::query_scalar(
                    "SELECT snapshot_id FROM migration_import WHERE id=$1 AND organization_id=$2",
                )
                .bind(parent)
                .bind(f.org)
                .fetch_one(&migrator)
                .await
                .unwrap();
                sqlx::query("UPDATE migration_snapshot_capture SET accepted=false,classification='no_progress' WHERE snapshot_id=$1 AND organization_id=$2 AND stream='people'")
                    .bind(baseline).bind(f.org).execute(&migrator).await.unwrap();
            }
            _ => unreachable!(),
        }
        let id = prepare_and_pause_after_source_fault(&f, report).await;
        let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap();
        assert_eq!(detail["state"], "paused", "{fault}: {detail}");
        assert_eq!(
            detail["pause_reason"], "source_integrity",
            "{fault}: {detail}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Adversarial source'",
            )
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            0,
            "{fault} must not create a Person"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1",
            )
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
            0,
            "{fault} must not fabricate an admission result"
        );
    }
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn huge_ids_and_unsupported_contact_shapes_are_held_before_confirmation(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    // The report boundary rejects a source identity beyond the frozen 128-digit
    // maximum before an admission run exists. It cannot be normalized into a
    // different executable identity.
    f.reader.set_records(
        Stream::People,
        vec![json!({"id":"9".repeat(129),"firstName":"Too long ID","stage":"Lead"})],
    );
    let connection =
        sqlx::query("SELECT id,revision FROM migration_connection WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let proposed = snapshot::propose(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        snapshot::ProposeCoreSnapshot {
            request_id: Uuid::new_v4(),
            connection_id: connection.get("id"),
            expected_revision: connection.get("revision"),
        },
    )
    .await
    .unwrap();
    let newer = Uuid::parse_str(proposed["snapshot"]["id"].as_str().unwrap()).unwrap();
    snapshot::source_action(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        newer,
        SnapshotRequest {
            request_id: Uuid::new_v4(),
        },
        SourceAction::Confirm,
    )
    .await
    .unwrap();
    import_support::drain_source(&f.pool, &f.key, f.reader.as_ref(), &f.policy).await;
    assert!(matches!(
        core_change_reports::prepare(
            &f.pool,
            &f.key,
            &f.policy,
            &f.ctx,
            core_change_reports::PrepareCoreChangeReport {
                request_id: Uuid::new_v4(),
                parent_import_id: parent,
                newer_snapshot_id: newer,
            },
            Some(&ReleaseReadiness::for_tests()),
        )
        .await,
        Err(MigrationError::SourceNotEligible)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_admission WHERE organization_id=$1 AND parent_import_id=$2",
        )
        .bind(f.org)
        .bind(parent)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "an oversized source identity must never acquire an admission run"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Too long ID'",
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );

    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id = db_people_admission_execution::ready(
        &f,
        parent,
        vec![
            json!({"id":107,"firstName":"Malformed contact","stage":"Lead","emails":[{"value":42}]}),
            json!({"id":108,"firstName":"Qualified control","stage":"Lead"}),
        ],
    )
    .await;
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(number(&detail["plan"]["counts"]["eligible"]), 1, "{detail}");
    assert_eq!(number(&detail["plan"]["counts"]["held"]), 1, "{detail}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Malformed contact'",
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0,
        "an unsupported contact shape must not create a Person during preview"
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn stale_confirmation_and_expired_preview_require_a_fresh_exact_plan(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id = db_people_admission_execution::ready(
        &f,
        parent,
        vec![json!({"id":106,"firstName":"Exact plan","stage":"Lead"})],
    )
    .await;
    let initial = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    let mut altered = confirmation(&initial, Uuid::new_v4());
    altered.plan_digest = "00".repeat(32);
    assert!(matches!(
        people_admission::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            altered,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let stale = confirmation(&initial, Uuid::new_v4());
    people_admission::repreview(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        people_admission::RepreviewPeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_plan_revision: stale.plan_revision,
        },
    )
    .await
    .unwrap();
    let current = seal_after_repreview(&f, id).await;
    assert!(matches!(
        people_admission::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            stale,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let current_plan = Uuid::parse_str(current["plan"]["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE migration_people_admission_plan SET state='expired',expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(current_plan)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(matches!(
        people_admission::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            confirmation(&current, Uuid::new_v4()),
            Some(&ReleaseReadiness::for_tests()),
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    let after = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(after["state"], "ready");
    assert!(sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT confirmed_admission_plan_id FROM migration_people_admission WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap()
    .is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_admission_receipt WHERE admission_id=$1 AND action='confirm'",
        )
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Exact plan'",
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn invalidated_stage_mapping_at_execution_pauses_without_native_write(migrator: PgPool) {
    let f = import_support::fixture(&migrator, import_support::default_people()).await;
    let parent = db_activity_source::completed_parent(&f).await;
    let id = db_people_admission_execution::ready(
        &f,
        parent,
        vec![json!({"id":106,"firstName":"Invalidated stage target","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let detail = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    let mapping: Uuid = sqlx::query_scalar(
        "SELECT stage_mapping_id FROM migration_people_admission_item WHERE admission_id=$1 AND disposition='eligible'",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    people_admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        confirmation(&detail, Uuid::new_v4()),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE migration_import_mapping SET target_id=$2 WHERE id=$1 AND organization_id=$3",
    )
    .bind(mapping)
    .bind(Uuid::new_v4())
    .bind(f.org)
    .execute(&migrator)
    .await
    .unwrap();
    assert!(matches!(
        people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await,
        Err(MigrationError::SourceNotEligible)
    ));
    let paused = people_admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    assert_eq!(paused["state"], "paused", "{paused}");
    assert_eq!(paused["pause_reason"], "source_integrity", "{paused}");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM person WHERE organization_id=$1 AND first_name='Invalidated stage target'",
        )
        .bind(f.org)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1",
        )
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        0
    );
}

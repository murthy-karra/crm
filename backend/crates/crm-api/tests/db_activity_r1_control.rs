//! R1 regressions: commands, readers and workers enforce every frozen boundary.
use crate::{db_activity_lifecycle as lifecycle, db_activity_source as source};
use crm_api::{
    auth::workspace,
    domain::migration::{
        activity::{self, ActivityAction, ActivityPage},
        activity_worker, MigrationError,
    },
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[sqlx::test(migrations = "./migrations")]
#[ignore]
async fn activity_rejects_changed_account_capture_and_exact_preview(migrator: PgPool) {
    let (f, parent, child, ready) = lifecycle::tasks(&migrator, 1).await;
    let frozen_parent = source::parent_state(&f, parent).await;
    let calls = f.reader.calls();
    let original = sqlx::query("SELECT source_account_id,capture_sequence,preview_id FROM migration_activity_import WHERE id=$1 AND organization_id=$2")
        .bind(child).bind(f.org).fetch_one(&migrator).await.unwrap();
    // A different preview of this same snapshot is structurally valid but is
    // not the exact completed parent's approved preview. Its copied encrypted
    // content is deliberately never read: boundary rejection must happen first.
    let different_preview = Uuid::new_v4();
    sqlx::query("INSERT INTO migration_snapshot_preview SELECT (jsonb_populate_record(NULL::migration_snapshot_preview,to_jsonb(p)||jsonb_build_object('id',$2::uuid))).* FROM migration_snapshot_preview p WHERE p.id=$1")
        .bind(original.get::<Uuid,_>("preview_id")).bind(different_preview).execute(&migrator).await.unwrap();
    for confirmed in [false, true] {
        if confirmed {
            activity::confirm(
                &f.pool,
                &f.key,
                &f.ctx,
                child,
                source::confirmation(&ready),
                &workspace::ReleaseReadiness::for_tests(),
                &f.policy,
            )
            .await
            .unwrap();
        }
        for field in ["source_account_id", "capture_sequence", "preview_id"] {
            match field {
                "source_account_id" => {
                    sqlx::query("UPDATE migration_activity_import SET source_account_id=source_account_id+1 WHERE id=$1").bind(child).execute(&migrator).await.unwrap();
                }
                "capture_sequence" => {
                    sqlx::query(
                        "UPDATE migration_activity_import SET capture_sequence=0 WHERE id=$1",
                    )
                    .bind(child)
                    .execute(&migrator)
                    .await
                    .unwrap();
                }
                _ => {
                    sqlx::query("UPDATE migration_activity_import SET preview_id=$2 WHERE id=$1")
                        .bind(child)
                        .bind(different_preview)
                        .execute(&migrator)
                        .await
                        .unwrap();
                }
            }
            assert!(
                matches!(
                    activity::detail(&f.pool, &f.key, &f.ctx, child, &f.policy).await,
                    Err(MigrationError::SourceNotEligible)
                ),
                "detail accepted {field}"
            );
            assert!(
                matches!(
                    activity::records(&f.pool, &f.key, &f.ctx, child, ActivityPage::default())
                        .await,
                    Err(MigrationError::SourceNotEligible)
                ),
                "records accepted {field}"
            );
            if confirmed {
                assert!(activity_worker::run_once(&f.pool, &f.key, &f.policy)
                    .await
                    .unwrap());
                let paused = sqlx::query(
                    "SELECT state,pause_reason FROM migration_activity_import WHERE id=$1",
                )
                .bind(child)
                .fetch_one(&migrator)
                .await
                .unwrap();
                assert_eq!(paused.get::<String, _>("state"), "paused");
                assert_eq!(
                    paused.get::<String, _>("pause_reason"),
                    "source_evidence_unavailable"
                );
                let writes: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM task WHERE organization_id=$1),(SELECT count(*) FROM migration_activity_result WHERE import_id=$2),(SELECT count(*) FROM migration_activity_identity WHERE import_id=$2)")
                    .bind(f.org).bind(child).fetch_one(&migrator).await.unwrap();
                assert_eq!(writes, (0, 0, 0), "worker executed {field}");
            } else {
                assert!(
                    matches!(
                        activity::confirm(
                            &f.pool,
                            &f.key,
                            &f.ctx,
                            child,
                            source::confirmation(&ready),
                            &workspace::ReleaseReadiness::for_tests(),
                            &f.policy
                        )
                        .await,
                        Err(MigrationError::SourceNotEligible)
                    ),
                    "confirm accepted {field}"
                );
            }
            sqlx::query("UPDATE migration_activity_import SET source_account_id=$2,capture_sequence=$3,preview_id=$4 WHERE id=$1")
                .bind(child).bind(original.get::<i64,_>("source_account_id")).bind(original.get::<i64,_>("capture_sequence")).bind(original.get::<Uuid,_>("preview_id")).execute(&migrator).await.unwrap();
            if confirmed {
                let restored = source::ready(&f, child).await;
                activity::action(
                    &f.pool,
                    &f.key,
                    &f.ctx,
                    child,
                    ActivityAction {
                        request_id: Uuid::new_v4(),
                        expected_revision: restored["revision"].as_str().unwrap().into(),
                    },
                    true,
                    &f.policy,
                )
                .await
                .unwrap();
            }
        }
    }
    source::drain(&f).await;
    let done = source::ready(&f, child).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["counts"]["tasks"]["applied"], "1");
    assert_eq!(source::parent_state(&f, parent).await, frozen_parent);
    assert_eq!(f.reader.calls(), calls);
    lifecycle::assert_bytes(&f, child).await;
}

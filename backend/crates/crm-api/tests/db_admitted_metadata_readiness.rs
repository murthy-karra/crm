//! Shared readiness rejects incomplete staged preparation before handover.
use crm_app::auth::workspace::ReleaseReadiness;
use sqlx::PgPool;

#[sqlx::test]
#[ignore]
async fn admitted_metadata_readiness_requires_complete_preparation_schema(pool: PgPool) {
    let release = ReleaseReadiness::for_tests();
    release
        .require_admitted_metadata(&mut pool.acquire().await.unwrap())
        .await
        .unwrap();
    for mutation in [
        "DROP TABLE migration_admitted_metadata_observation",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN preparation_phase",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN preparation_bytes",
        "ALTER TABLE migration_admitted_metadata_source DROP COLUMN qualified",
        "ALTER TABLE migration_admitted_metadata_manifest DROP COLUMN expected_person_id",
        "ALTER TABLE migration_admitted_metadata_manifest ALTER COLUMN person_id SET NOT NULL",
        "ALTER TABLE migration_admitted_metadata_operation DISABLE TRIGGER admitted_metadata_preparation_bytes",
        "ALTER TABLE migration_admitted_metadata_operation ENABLE REPLICA TRIGGER admitted_metadata_preparation_bytes",
        "ALTER TABLE migration_metadata_identity ENABLE REPLICA TRIGGER migration_metadata_identity_claim_guard",
        "DROP INDEX migration_admitted_metadata_source_work",
        "DROP INDEX migration_admitted_metadata_source_order",
        "DROP INDEX migration_admitted_metadata_admission_work",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN snapshot_id",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN source_output_revision",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN previous_plan_id",
        "ALTER TABLE migration_admitted_metadata_plan ALTER COLUMN snapshot_id DROP NOT NULL",
        "ALTER TABLE migration_admitted_metadata_plan ALTER COLUMN previous_plan_id SET NOT NULL",
        "ALTER TABLE migration_admitted_metadata_receipt DROP COLUMN snapshot_id",
        "ALTER TABLE migration_admitted_metadata_receipt ALTER COLUMN snapshot_id DROP NOT NULL",
        "DROP INDEX migration_admitted_metadata_selected_target",
        "DROP INDEX migration_admitted_metadata_mapping_prepare",
        "ALTER TABLE migration_admitted_metadata_plan DROP CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check",
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(mutation).execute(&mut *tx).await.unwrap();
        let error = release
            .require_admitted_metadata(&mut tx)
            .await
            .expect_err(mutation);
        assert!(
            error
                .to_string()
                .contains("admitted metadata schema incompatible"),
            "{mutation}: {error}"
        );
        tx.rollback().await.unwrap();
    }
    release
        .require_admitted_metadata(&mut pool.acquire().await.unwrap())
        .await
        .unwrap();
}

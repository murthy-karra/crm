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
        "ALTER TABLE migration_admitted_metadata_manifest DROP COLUMN oversized",
        "ALTER TABLE migration_admitted_metadata_manifest ALTER COLUMN oversized DROP NOT NULL",
        "ALTER TABLE migration_admitted_metadata_mapping DROP COLUMN field_name_key",
        "ALTER TABLE migration_admitted_metadata_mapping ALTER COLUMN field_name_key SET NOT NULL",
        "ALTER TABLE migration_admitted_metadata_mapping DROP CONSTRAINT migration_admitted_metadata_mapping_field_name_key_check",
        "ALTER TABLE migration_admitted_metadata_mapping DROP CONSTRAINT migration_admitted_metadata_mapping_field_name_key_check; ALTER TABLE migration_admitted_metadata_mapping ADD CONSTRAINT migration_admitted_metadata_mapping_field_name_key_check CHECK(field_name_key IS NULL OR octet_length(field_name_key)=31)",
        "DROP INDEX migration_admitted_metadata_field_name",
        "DROP INDEX migration_admitted_metadata_field_name; CREATE INDEX migration_admitted_metadata_field_name ON migration_admitted_metadata_mapping(plan_id,organization_id,field_name_key,id)",
        "ALTER TABLE migration_admitted_metadata_plan DROP CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check; ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check CHECK(preparation_phase IN ('sources','fields','options','tags','choices','cohort','values','seal','complete','remainder_mappings','remainder_people','remainder_operations'))",
        "DO $test$ DECLARE definition TEXT; BEGIN SELECT pg_get_functiondef('crm_admitted_metadata_preparation_bytes()'::regprocedure) INTO definition; EXECUTE replace(definition,'+COALESCE(octet_length(NEW.field_name_key),0)',''); END $test$;",
        "DO $test$ DECLARE definition TEXT; BEGIN SELECT pg_get_functiondef('crm_admitted_metadata_preparation_bytes()'::regprocedure) INTO definition; EXECUTE replace(definition,'+COALESCE(octet_length(OLD.field_name_key),0)',''); END $test$;",
        "DROP INDEX migration_admitted_metadata_mapping_parent",
        "DROP INDEX migration_admitted_metadata_plan_building",
        "DROP TABLE migration_admitted_metadata_observation",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN remainder_plan_id",
        "ALTER TABLE migration_admitted_metadata_plan DROP COLUMN evidence_plan_id",
        "ALTER TABLE migration_admitted_metadata_plan ALTER COLUMN remainder_plan_id SET NOT NULL",
        "ALTER TABLE migration_admitted_metadata_mapping DROP COLUMN execute_unit",
        "ALTER TABLE migration_admitted_metadata_mapping ALTER COLUMN execute_unit DROP NOT NULL",
        "ALTER TABLE migration_admitted_metadata_mapping DROP COLUMN dependency_result_id",
        "ALTER TABLE migration_admitted_metadata_manifest DROP COLUMN predecessor_manifest_id",
        "ALTER TABLE migration_admitted_metadata_import DROP COLUMN held_settled_people",
        "DROP INDEX migration_admitted_metadata_mapping_predecessor",
        "DROP INDEX migration_admitted_metadata_manifest_predecessor",
        "DROP INDEX migration_admitted_metadata_mapping_execute",
        "ALTER TABLE migration_admitted_metadata_plan DROP CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check; ALTER TABLE migration_admitted_metadata_plan ADD CONSTRAINT migration_admitted_metadata_plan_preparation_phase_check CHECK(preparation_phase IN ('sources','fields','options','tags','choices','cohort','values','seal','complete'))",
        "DROP TABLE migration_admitted_metadata_alias",
        "ALTER TABLE migration_admitted_metadata_import DROP COLUMN admission_plan_id",
        "ALTER TABLE migration_admitted_metadata_import ALTER COLUMN admission_plan_id DROP NOT NULL",
        "ALTER TABLE migration_admitted_metadata_import DROP COLUMN settled_eligible_people",
        "ALTER TABLE migration_admitted_metadata_import DROP COLUMN counts",
        "ALTER TABLE migration_admitted_metadata_mapping DROP COLUMN dependent_count",
        "ALTER TABLE migration_admitted_metadata_mapping DROP COLUMN alias_count",
        "ALTER TABLE migration_admitted_metadata_alias DISABLE TRIGGER admitted_metadata_mapping_counts",
        "ALTER TABLE migration_admitted_metadata_operation ENABLE REPLICA TRIGGER admitted_metadata_mapping_counts",
        "DROP INDEX migration_admitted_metadata_alias_page",
        "DROP INDEX migration_admitted_metadata_result_filtered_page",
        "DROP INDEX migration_admitted_metadata_person_result_page",
        "DROP INDEX migration_admitted_metadata_cohort_seek",
        "DROP INDEX migration_admitted_metadata_one_root; CREATE UNIQUE INDEX migration_admitted_metadata_one_root ON migration_admitted_metadata_import(organization_id,admission_id) WHERE predecessor_import_id IS NULL",
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
        sqlx::raw_sql(mutation).execute(&mut *tx).await.unwrap();
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

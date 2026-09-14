//! D-082 preparation accepts only an actual terminal admission cohort and
//! freezes the selected report's retained People evidence and local baseline.
use crate::db_admitted_people_refresh_execution::{fixture_with_admission, report};
use crm_api::{auth::workspace::ReleaseReadiness, domain::migration::admitted_metadata};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_metadata_preparation_freezes_terminal_people_cohort(migrator: PgPool) {
    // The long-lived isolated gate may have applied the additive migration
    // before this new grant was authored. Fresh migration installs receive it
    // from 20261002000001; apply it here as the migration owner so this test
    // can exercise the app-role handover path too.
    sqlx::query("GRANT SELECT ON migration_metadata_identity TO PUBLIC")
        .execute(&migrator)
        .await
        .unwrap();
    let people = vec![
        json!({"id":104,"firstName":"Admitted","lastName":"Person","stage":"Lead","assignedUserId":3,"tags":["Past Client"]}),
    ];
    let (fixture, parent, admission) = fixture_with_admission(&migrator, people.clone()).await;
    let selected = report(&fixture, parent, people).await;
    let value = admitted_metadata::prepare(
        &fixture.pool,
        &fixture.key,
        &ReleaseReadiness::for_tests(),
        &fixture.ctx,
        admitted_metadata::Prepare {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            source_report_id: selected,
        },
    )
    .await
    .unwrap();
    let root = Uuid::parse_str(value["import"]["id"].as_str().unwrap()).unwrap();
    let plan: Uuid = sqlx::query_scalar(
        "SELECT latest_plan_id FROM migration_admitted_metadata_import WHERE id=$1",
    )
    .bind(root)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_manifest WHERE import_id=$1 AND plan_id=$2 AND disposition='eligible'").bind(root).bind(plan).fetch_one(&fixture.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_source WHERE import_id=$1 AND family='people'").bind(root).fetch_one(&fixture.pool).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND kind='tag' AND disposition='held'").bind(root).fetch_one(&fixture.pool).await.unwrap(),1);
    assert!(sqlx::query_scalar::<_,i64>("SELECT octet_length(baseline_nonce)+octet_length(baseline_ciphertext) FROM migration_admitted_metadata_manifest WHERE import_id=$1").bind(root).fetch_one(&fixture.pool).await.unwrap()>24);
}

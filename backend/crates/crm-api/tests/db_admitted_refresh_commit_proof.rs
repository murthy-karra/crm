//! Final-review regressions through genuine retained import/admission/report fixtures.
//! Migrator writes below explicitly model concurrent edits and erased evidence.
use crate::{db_admitted_people_refresh_execution as execution, import_support::Fixture};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{admitted_people_refresh as refresh, admitted_people_refresh_worker},
};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn id(value: &Value) -> Uuid {
    Uuid::parse_str(value.as_str().unwrap()).unwrap()
}
fn count(value: &Value) -> i64 {
    value.as_str().unwrap().parse().unwrap()
}
async fn drain(f: &Fixture) {
    for _ in 0..100 {
        if !admitted_people_refresh_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap()
        {
            return;
        }
    }
    panic!("bounded fixture did not drain");
}
async fn prepare(f: &Fixture, admission: Uuid, report: Uuid) -> (Uuid, Value) {
    let receipt = refresh::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        refresh::PrepareAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            admission_id: admission,
            report_id: report,
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let run = id(&receipt["refresh_id"]);
    drain(f).await;
    let detail = refresh::detail(&f.pool, &f.key, &f.ctx, run).await.unwrap();
    assert_eq!(detail["state"], "ready", "{detail}");
    (run, detail)
}
async fn confirm(f: &Fixture, run: Uuid, detail: &Value) {
    let p = &detail["plan"];
    refresh::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        run,
        refresh::ConfirmAdmittedPeopleRefresh {
            request_id: Uuid::new_v4(),
            plan_id: id(&p["id"]),
            plan_revision: count(&p["revision"]),
            plan_digest: p["digest"].as_str().unwrap().to_owned(),
            acknowledged_eligible_count: count(&p["counts"]["eligible"]),
            acknowledged_coverage: true,
            acknowledged_exclusions: true,
            acknowledged_name_clears: count(&p["counts"]["name_clears"]),
            acknowledged_assignment_clears: count(&p["counts"]["assignment_clears"]),
            acknowledged_contact_removals: count(&p["counts"]["contact_removals"]),
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
}
fn person(source: i32, name: &str) -> Value {
    json!({"id":source,"firstName":name,"stage":"Lead","assignedUserId":3})
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn missing_initial_identity_holds_only_that_person_without_seeding_baseline(
    migrator: PgPool,
) {
    let (f, parent, admission) = execution::fixture_with_admission(
        &migrator,
        vec![
            person(104, "Missing identity"),
            person(105, "Healthy identity"),
        ],
    )
    .await;
    let report = execution::report(
        &f,
        parent,
        vec![
            person(104, "Untrusted proposal"),
            person(105, "Healthy update"),
        ],
    )
    .await;
    sqlx::query("DELETE FROM migration_import_identity WHERE organization_id=$1 AND admission_id=$2 AND source_id='104'")
        .bind(f.org).bind(admission).execute(&migrator).await.unwrap();
    let (run, detail) = prepare(&f, admission, report).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1");
    assert_eq!(detail["plan"]["counts"]["held"], "1");
    let baselines: i64 = sqlx::query_scalar("SELECT count(*) FROM migration_admitted_people_refresh_baseline WHERE admission_id=$1 AND source_id='104'")
        .bind(admission).fetch_one(&migrator).await.unwrap();
    assert_eq!(baselines, 0, "an unproved identity cannot seed B");
    confirm(&f, run, &detail).await;
    drain(&f).await;
    let outcomes: Vec<(String,String)> = sqlx::query_as("SELECT source_id,disposition FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 ORDER BY source_id")
        .bind(run).fetch_all(&migrator).await.unwrap();
    assert_eq!(
        outcomes,
        vec![
            ("104".into(), "held_evidence_gap".into()),
            ("105".into(), "settled".into())
        ]
    );
    let names: Vec<(String,String)> = sqlx::query_as("SELECT ar.source_id,p.first_name FROM migration_people_admission_result ar JOIN person p ON p.id=ar.person_id AND p.organization_id=ar.organization_id WHERE ar.admission_id=$1 ORDER BY ar.source_id")
        .bind(admission).fetch_all(&migrator).await.unwrap();
    assert_eq!(
        names,
        vec![
            ("104".into(), "Missing identity".into()),
            ("105".into(), "Healthy update".into())
        ]
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn already_current_rechecks_native_identity_and_exact_retained_source(migrator: PgPool) {
    let original: Vec<_> = (104..=108).map(|n| person(n, "Original")).collect();
    let (f, parent, admission) =
        execution::fixture_with_admission(&migrator, original.clone()).await;
    let mut newer = original;
    newer[0] = person(104, "Eligible update");
    let report = execution::report(&f, parent, newer).await;
    let (run, detail) = prepare(&f, admission, report).await;
    assert_eq!(detail["plan"]["counts"]["eligible"], "1");
    assert_eq!(detail["plan"]["counts"]["already_current"], "4");
    confirm(&f, run, &detail).await;
    sqlx::query("UPDATE person SET first_name='Concurrent edit' WHERE id=(SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='105')")
        .bind(admission).execute(&migrator).await.unwrap();
    sqlx::query("DELETE FROM migration_import_identity WHERE organization_id=$1 AND admission_id=$2 AND source_id='106'")
        .bind(f.org).bind(admission).execute(&migrator).await.unwrap();
    // Erase only this indexed observation; retain the encrypted source page.
    sqlx::query("DELETE FROM migration_snapshot_record WHERE snapshot_id=(SELECT newer_snapshot_id FROM migration_admitted_people_refresh WHERE id=$1) AND family='people' AND source_id='107'")
        .bind(run).execute(&migrator).await.unwrap();
    drain(&f).await;
    let outcomes: Vec<(String,String)> = sqlx::query_as("SELECT source_id,disposition FROM migration_admitted_people_refresh_result WHERE refresh_id=$1 ORDER BY source_id")
        .bind(run).fetch_all(&migrator).await.unwrap();
    assert_eq!(
        outcomes,
        vec![
            ("104".into(), "settled".into()),
            ("105".into(), "held_stale".into()),
            ("106".into(), "held_stale".into()),
            ("107".into(), "held_stale".into()),
            ("108".into(), "settled_noop".into())
        ]
    );
    let advanced: Vec<String> = sqlx::query_scalar("SELECT source_id FROM migration_admitted_people_refresh_baseline WHERE admission_id=$1 AND result_id IS NOT NULL ORDER BY source_id")
        .bind(admission).fetch_all(&migrator).await.unwrap();
    assert_eq!(
        advanced,
        vec!["104", "108"],
        "only revalidated items advance B"
    );
    let held_names: Vec<String> = sqlx::query_scalar("SELECT p.first_name FROM person p JOIN migration_people_admission_result ar ON ar.person_id=p.id AND ar.organization_id=p.organization_id WHERE ar.admission_id=$1 AND ar.source_id IN ('105','106','107') ORDER BY ar.source_id")
        .bind(admission).fetch_all(&migrator).await.unwrap();
    assert_eq!(held_names, vec!["Concurrent edit", "Original", "Original"]);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn omitted_stage_and_assignment_preserve_initial_and_later_baselines(migrator: PgPool) {
    let (f, parent, admission) =
        execution::fixture_with_admission(&migrator, vec![person(104, "Initial")]).await;
    async fn admission_evidence(pool: &PgPool, org: Uuid) -> Vec<(String, i64, String)> {
        sqlx::query_as("SELECT 'person_admission_provenance'::text,count(*),md5(coalesce(string_agg(to_jsonb(p)::text,E'\n' ORDER BY to_jsonb(p)::text),'')) FROM person_admission_provenance p WHERE organization_id=$1 UNION ALL SELECT 'person_admitted'::text,count(*),md5(coalesce(string_agg(to_jsonb(p)::text,E'\n' ORDER BY to_jsonb(p)::text),'')) FROM person_admitted p WHERE organization_id=$1")
            .bind(org).fetch_all(pool).await.unwrap()
    }
    let original_evidence = admission_evidence(&migrator, f.org).await;
    for name in ["First refresh", "Second refresh"] {
        let mut observation = json!({"id":104,"firstName":name});
        if name == "First refresh" {
            observation["emails"] = json!([
                {"value":"first@synthetic.test","isPrimary":0},
                {"value":"priority@synthetic.test","isPrimary":1}
            ]);
        }
        let report = execution::report(&f, parent, vec![observation]).await;
        let (run, detail) = prepare(&f, admission, report).await;
        assert_eq!(detail["plan"]["counts"]["eligible"], "1");
        confirm(&f, run, &detail).await;
        drain(&f).await;
        let result: String = sqlx::query_scalar(
            "SELECT disposition FROM migration_admitted_people_refresh_result WHERE refresh_id=$1",
        )
        .bind(run)
        .fetch_one(&migrator)
        .await
        .unwrap();
        assert_eq!(result, "settled", "{name}");
        let native: (String,Uuid,Option<Uuid>)=sqlx::query_as("SELECT p.first_name,p.stage_id,p.assigned_user_id FROM person p JOIN migration_people_admission_result ar ON ar.person_id=p.id AND ar.organization_id=p.organization_id WHERE ar.admission_id=$1 AND ar.source_id='104'")
            .bind(admission).fetch_one(&migrator).await.unwrap();
        assert_eq!(native, (name.to_owned(), f.lead_stage, Some(f.member)));
        let contacts: Vec<String> = sqlx::query_scalar("SELECT c.value FROM contact_method c JOIN migration_people_admission_result ar ON ar.person_id=c.person_id AND ar.organization_id=c.organization_id WHERE ar.admission_id=$1 AND ar.source_id='104' ORDER BY c.import_order")
            .bind(admission).fetch_all(&migrator).await.unwrap();
        assert_eq!(
            contacts,
            vec!["priority@synthetic.test", "first@synthetic.test"],
            "numeric primary is first and omitted later contacts preserve order"
        );
        assert_eq!(
            admission_evidence(&migrator, f.org).await,
            original_evidence,
            "original admission provenance and immutable fact bytes remain unchanged"
        );
    }
}

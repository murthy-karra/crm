//! Synthetic retained capture journeys through both real refresh workers.
use crate::{
    db_admitted_people_refresh_execution as admitted, db_people_refresh_execution as original,
    import_support::Fixture,
};
use crm_api::{
    auth::workspace::ReleaseReadiness,
    domain::migration::{
        admitted_people_refresh as a, admitted_people_refresh_worker as aw,
        people_mapping_repair::Owner, people_mapping_repair_commands as commands,
        people_mapping_repair_queries as queries, people_refresh as o, people_refresh_worker as ow,
        MigrationError,
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;
fn id(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}
fn n(v: &Value) -> i64 {
    v.as_str().unwrap().parse().unwrap()
}
async fn detail(f: &Fixture, owner: Owner, run: Uuid) -> Value {
    match owner {
        Owner::Original => o::detail(&f.pool, &f.key, &f.ctx, run).await,
        Owner::Admitted => a::detail(&f.pool, &f.key, &f.ctx, run).await,
    }
    .unwrap()
}
async fn until(f: &Fixture, owner: Owner, run: Uuid, state: &str) -> Value {
    for _ in 0..100 {
        let d = detail(f, owner, run).await;
        if d["state"] == state {
            return d;
        }
        assert!(match owner {
            Owner::Original =>
                ow::run_once(
                    &f.pool,
                    &f.key,
                    &f.policy,
                    Some(&ReleaseReadiness::for_tests())
                )
                .await,
            Owner::Admitted =>
                aw::run_once(
                    &f.pool,
                    &f.key,
                    &f.policy,
                    Some(&ReleaseReadiness::for_tests())
                )
                .await,
        }
        .unwrap());
    }
    panic!("worker did not reach {state}")
}
async fn repair_preview(
    f: &Fixture,
    owner: Owner,
    source: Uuid,
    source_detail: &Value,
    target: Uuid,
) -> (Uuid, Value) {
    let request_id = Uuid::new_v4();
    let make = || commands::Prepare {
        request_id,
        report_id: id(&source_detail["report_id"]),
        expected_lifecycle_revision: n(&source_detail["lifecycle_revision"]),
        anchor: commands::Anchor::Preview {
            plan_id: id(&source_detail["plan"]["id"]),
            plan_revision: n(&source_detail["plan"]["revision"]),
        },
    };
    let created = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        owner,
        source,
        make(),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let replay = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        owner,
        source,
        make(),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert_eq!(created, replay);
    assert_eq!(detail(f, owner, source).await["state"], "cancelled");
    let run = id(&created["refresh_id"]);
    let d = until(f, owner, run, "paused").await;
    assert_eq!(d["pause_reason"], "awaiting_mapping_choices");
    let retry = match owner {
        Owner::Original => {
            o::retry(
                &f.pool,
                &f.key,
                &f.ctx,
                run,
                o::LifecyclePeopleRefresh {
                    request_id: Uuid::new_v4(),
                    expected_lifecycle_revision: n(&d["lifecycle_revision"]),
                },
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
        }
        Owner::Admitted => {
            a::retry(
                &f.pool,
                &f.key,
                &f.ctx,
                run,
                a::LifecycleAdmittedPeopleRefresh {
                    request_id: Uuid::new_v4(),
                    expected_lifecycle_revision: n(&d["lifecycle_revision"]),
                },
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
        }
    };
    assert!(matches!(retry, Err(MigrationError::Conflict)));
    let page = queries::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        owner,
        run,
        queries::Page::default(),
    )
    .await
    .unwrap();
    let stage = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["kind"] == "stage")
        .unwrap();
    let choice_request = Uuid::new_v4();
    let choices = || commands::EditChoices {
        request_id: choice_request,
        expected_draft_revision: 0,
        choices: vec![commands::Choice {
            key_id: id(&stage["id"]),
            disposition: "existing".into(),
            target_id: Some(target),
        }],
    };
    let saved = commands::edit_choices(&f.pool, &f.key, &f.policy, &f.ctx, owner, run, choices())
        .await
        .unwrap();
    assert_eq!(
        commands::edit_choices(&f.pool, &f.key, &f.policy, &f.ctx, owner, run, choices())
            .await
            .unwrap(),
        saved
    );
    (run, seal(f, owner, run).await)
}
async fn seal(f: &Fixture, owner: Owner, run: Uuid) -> Value {
    let d = detail(f, owner, run).await;
    match owner {
        Owner::Original => {
            o::repreview(
                &f.pool,
                &f.key,
                &f.ctx,
                run,
                o::RepreviewPeopleRefresh {
                    request_id: Uuid::new_v4(),
                    expected_plan_revision: n(&d["lifecycle_revision"]),
                },
            )
            .await
        }
        Owner::Admitted => {
            a::repreview(
                &f.pool,
                &f.key,
                &f.ctx,
                run,
                a::RepreviewAdmittedPeopleRefresh {
                    request_id: Uuid::new_v4(),
                    expected_plan_revision: n(&d["lifecycle_revision"]),
                },
            )
            .await
        }
    }
    .unwrap();
    until(f, owner, run, "ready").await
}
async fn confirm(f: &Fixture, owner: Owner, run: Uuid, d: &Value) {
    let mut body = original::confirmation(d);
    body["mapping_repair"] = json!({"choices_digest":d["plan"]["mapping_repair"]["choices_digest"],"candidate_count":n(&d["plan"]["mapping_repair"]["candidate_count"]),"approval_only_count":n(&d["plan"]["mapping_repair"]["approval_only_count"]),"unassigned_count":n(&d["plan"]["mapping_repair"]["unassigned_count"])});
    match owner {
        Owner::Original => {
            o::confirm(
                &f.pool,
                &f.key,
                &f.ctx,
                run,
                serde_json::from_value(body).unwrap(),
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
        }
        Owner::Admitted => {
            body["acknowledged_eligible_count"] = json!(n(&d["plan"]["counts"]["eligible"]));
            a::confirm(
                &f.pool,
                &f.key,
                &f.ctx,
                run,
                serde_json::from_value(body).unwrap(),
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
        }
    }
    .unwrap();
}
async fn ledger(f: &Fixture, owner: Owner, run: Uuid) {
    let p = if owner == Owner::Original {
        "migration_people_refresh"
    } else {
        "migration_admitted_people_refresh"
    };
    let source = if owner == Owner::Admitted {
        "octet_length(source_key)+COALESCE(octet_length(source_id),0)+COALESCE(octet_length(source_semantic_hmac),0)+"
    } else {
        ""
    };
    let source_result = if owner == Owner::Admitted {
        "octet_length(source_id)+"
    } else {
        ""
    };
    let receipt = if owner == Owner::Admitted {
        "octet_length(digest)+"
    } else {
        ""
    };
    let sql=format!("SELECT COALESCE((SELECT sum(octet_length(inputs_nonce)+octet_length(inputs_ciphertext)+COALESCE(octet_length(digest),0)+COALESCE(octet_length(repair_choices_digest),0)) FROM {p}_plan WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum({source}octet_length(proposed_nonce)+octet_length(proposed_ciphertext)+octet_length(baseline_nonce)+octet_length(baseline_ciphertext)+octet_length(current_nonce)+octet_length(current_ciphertext)+octet_length(instructions_nonce)+octet_length(instructions_ciphertext)+COALESCE(octet_length(mapping_evidence_nonce),0)+COALESCE(octet_length(mapping_evidence_ciphertext),0)+COALESCE(octet_length(native_fingerprint),0)+COALESCE(octet_length(repair_stage_source_hmac),0)+COALESCE(octet_length(repair_assignee_source_hmac),0)) FROM {p}_item WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum(octet_length(value_nonce)+octet_length(value_ciphertext)) FROM {p}_contact WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum({source_result}octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM {p}_result WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum({receipt}octet_length(nonce)+octet_length(ciphertext)) FROM {p}_receipt WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum({source_result}octet_length(projection_nonce)+octet_length(projection_ciphertext)) FROM {p}_baseline WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key_hmac)) FROM {p}_repair_key WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum(octet_length(nonce)+octet_length(ciphertext)+octet_length(source_key_hmac)) FROM {p}_repair_choice WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum(octet_length(source_id)+COALESCE(octet_length(stage_source_hmac),0)+COALESCE(octet_length(assignee_source_hmac),0)) FROM {p}_repair_candidate WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum(octet_length(source_id)+octet_length(source_key_hmac)) FROM {p}_mapping_binding WHERE refresh_id=$1),0)
        +COALESCE((SELECT sum(octet_length(h.source_key_hmac)) FROM {p}_mapping_head h JOIN {p}_mapping_binding b ON b.id=h.binding_id WHERE b.refresh_id=$1),0)
        +COALESCE((SELECT octet_length(repair_choices_digest) FROM {p} WHERE id=$1),0)::bigint");
    let mut actual: i64 = sqlx::query_scalar(&sql)
        .bind(run)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    if owner == Owner::Admitted {
        let extra:i64=sqlx::query_scalar("SELECT COALESCE((SELECT sum(octet_length(before_nonce)+octet_length(before_ciphertext)+octet_length(after_nonce)+octet_length(after_ciphertext)) FROM person_admitted_refresh_provenance WHERE refresh_id=$1),0)+COALESCE((SELECT octet_length(preparation_checkpoint_key) FROM migration_admitted_people_refresh WHERE id=$1),0)::bigint").bind(run).fetch_one(&f.pool).await.unwrap();
        actual += extra;
    }
    assert_eq!(actual, n(&detail(f, owner, run).await["retained_bytes"]));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn original_all_held_repair_approval_only_and_normal_refresh_follow_through(
    migrator: PgPool,
) {
    let (f, parent, _) = original::mapped_fixture(&migrator).await;
    let mut people = original::rich_people();
    for p in &mut people {
        p["stage"] = json!("New source stage");
    }
    let (source, d) = original::ready(&f, parent, people.clone()).await;
    assert_eq!(d["plan"]["counts"]["eligible"], "0");
    let (run, preview) = repair_preview(&f, Owner::Original, source, &d, f.lead_stage).await;
    assert_eq!(
        preview["plan"]["mapping_repair"]["approval_only_count"],
        "2"
    );
    assert_eq!(preview["plan"]["counts"]["eligible"], "0");
    let no_ack: o::ConfirmPeopleRefresh =
        serde_json::from_value(original::confirmation(&preview)).unwrap();
    assert!(matches!(
        o::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            no_ack,
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    confirm(&f, Owner::Original, run, &preview).await;
    until(&f, Owner::Original, run, "completed").await;
    ledger(&f, Owner::Original, run).await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM migration_people_refresh_mapping_binding WHERE refresh_id=$1",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    // A legacy no-op worker cannot touch a repaired Organization.
    let mut tx = f.pool.begin().await.unwrap();
    let rejected =
        sqlx::query("UPDATE migration_people_refresh SET updated_at=clock_timestamp() WHERE id=$1")
            .bind(run)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert_eq!(
        rejected.as_database_error().unwrap().code().as_deref(),
        Some("P010R")
    );
    tx.rollback().await.unwrap();
    people[0]["firstName"] = json!("Fresh retained name");
    let (next, next_preview) = original::ready(&f, parent, people).await;
    assert_eq!(next_preview["plan"]["counts"]["eligible"], "1");
    assert_eq!(next_preview["plan"]["counts"]["already_current"], "1");
    // A no-op Person receives a local edit after preview: it must hold without moving B.
    let person:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_refresh_item WHERE refresh_id=$1 AND disposition='already_current'").bind(next).fetch_one(&f.pool).await.unwrap();
    sqlx::query("UPDATE person SET first_name='Local edit after preview' WHERE id=$1")
        .bind(person)
        .execute(&migrator)
        .await
        .unwrap();
    original::confirm(&f, next, &original::confirmation(&next_preview)).await;
    original::drain(&f, next).await;
    let outcomes:Vec<String>=sqlx::query_scalar("SELECT disposition FROM migration_people_refresh_result WHERE refresh_id=$1 ORDER BY disposition").bind(next).fetch_all(&f.pool).await.unwrap();
    assert_eq!(outcomes, vec!["held_stale", "settled"]);
    let head: Uuid = sqlx::query_scalar(
        "SELECT refresh_id FROM migration_people_refresh_baseline WHERE person_id=$1",
    )
    .bind(person)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(head, run);
    ledger(&f, Owner::Original, run).await;
    ledger(&f, Owner::Original, next).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_all_held_mapping_repair_uses_its_own_bindings(migrator: PgPool) {
    let mut people = vec![
        json!({"id":104,"firstName":"Admitted","stage":"Lead","assignedUserId":3,"emails":[],"phones":[]}),
    ];
    let (f, parent, admission) = admitted::fixture_with_admission(&migrator, people.clone()).await;
    people[0]["stage"] = json!("New admitted stage");
    people[0]["firstName"] = json!("Refreshed admitted");
    let report = admitted::report(&f, parent, people).await;
    let (source, d) = admitted::prepare(&f, admission, report).await;
    let (run, preview) = repair_preview(&f, Owner::Admitted, source, &d, f.lead_stage).await;
    assert_eq!(preview["plan"]["counts"]["eligible"], "1");
    confirm(&f, Owner::Admitted, run, &preview).await;
    until(&f, Owner::Admitted, run, "completed").await;
    ledger(&f, Owner::Admitted, run).await;
    let row=sqlx::query("SELECT count(*) AS count,(SELECT count(*) FROM migration_people_refresh_mapping_binding WHERE organization_id=$1) AS original_count FROM migration_admitted_people_refresh_mapping_binding WHERE refresh_id=$2 AND organization_id=$1").bind(f.org).bind(run).fetch_one(&f.pool).await.unwrap();
    assert_eq!(row.get::<i64, _>("count"), 1);
    assert_eq!(row.get::<i64, _>("original_count"), 0);
}

async fn repeated_repair_and_cancelled_remainder(migrator: PgPool, owner: Owner, hold_first: bool) {
    let (f, parent, cohort, mut people) = match owner {
        Owner::Original => {
            let (f, parent, _) = original::mapped_fixture(&migrator).await;
            (f, parent, parent, original::rich_people())
        }
        Owner::Admitted => {
            let people = vec![
                json!({"id":104,"firstName":"Admitted one","stage":"Lead","assignedUserId":3,"emails":[],"phones":[]}),
                json!({"id":105,"firstName":"Admitted two","stage":"Lead","assignedUserId":3,"emails":[],"phones":[]}),
            ];
            let (f, parent, cohort) =
                admitted::fixture_with_admission(&migrator, people.clone()).await;
            (f, parent, cohort, people)
        }
    };
    for person in &mut people {
        person["stage"] = json!("Repair source stage");
    }
    let (source, d) = fresh(&f, owner, parent, cohort, people.clone()).await;
    let (first, preview) = repair_preview(&f, owner, source, &d, f.lead_stage).await;
    confirm(&f, owner, first, &preview).await;
    until(&f, owner, first, "completed").await;
    // Renaming invalidates the immutable target snapshot without changing core IDs.
    sqlx::query("UPDATE stage SET name='Renamed Lead' WHERE id=$1 AND organization_id=$2")
        .bind(f.lead_stage)
        .bind(f.org)
        .execute(&migrator)
        .await
        .unwrap();
    let (blocked, d) = fresh(&f, owner, parent, cohort, people.clone()).await;
    assert_eq!(d["plan"]["counts"]["held"], "2");
    let (second, preview) = repair_preview(&f, owner, blocked, &d, f.lead_stage).await;
    confirm(&f, owner, second, &preview).await;
    until(&f, owner, second, "running").await;
    if hold_first {
        let p = if owner == Owner::Original {
            "migration_people_refresh"
        } else {
            "migration_admitted_people_refresh"
        };
        sqlx::query(&format!("UPDATE person SET first_name='Local edit before cancellation' WHERE id=(SELECT person_id FROM {p}_item WHERE refresh_id=$1 ORDER BY id LIMIT 1)")).bind(second).execute(&migrator).await.unwrap();
    }
    tick(&f, owner).await; // One settled success or hold, then an unfinished remainder.
    let d = detail(&f, owner, second).await;
    match owner {
        Owner::Original => {
            o::cancel(
                &f.pool,
                &f.key,
                &f.ctx,
                second,
                o::LifecyclePeopleRefresh {
                    request_id: Uuid::new_v4(),
                    expected_lifecycle_revision: n(&d["lifecycle_revision"]),
                },
            )
            .await
        }
        Owner::Admitted => {
            a::cancel(
                &f.pool,
                &f.key,
                &f.ctx,
                second,
                a::LifecycleAdmittedPeopleRefresh {
                    request_id: Uuid::new_v4(),
                    expected_lifecycle_revision: n(&d["lifecycle_revision"]),
                },
            )
            .await
        }
    }
    .unwrap();
    let d = detail(&f, owner, second).await;
    assert_eq!(d["mapping_repair_available"], true);
    let made = commands::prepare(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        owner,
        second,
        commands::Prepare {
            request_id: Uuid::new_v4(),
            report_id: id(&d["report_id"]),
            expected_lifecycle_revision: n(&d["lifecycle_revision"]),
            anchor: commands::Anchor::Results {
                plan_id: id(&preview["plan"]["id"]),
            },
        },
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    let remainder = id(&made["refresh_id"]);
    let draft = until(&f, owner, remainder, "paused").await;
    assert_eq!(draft["repair_candidate_count"], "1");
    assert_eq!(draft["repair_draft_revision"], "1");
    let page = queries::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        owner,
        remainder,
        queries::Page::default(),
    )
    .await
    .unwrap();
    assert!(page["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["kind"] == "stage" && i["choice"]["target_id"] == f.lead_stage.to_string()));
    let preview = seal(&f, owner, remainder).await;
    confirm(&f, owner, remainder, &preview).await;
    until(&f, owner, remainder, "completed").await;
    for run in [first, second, remainder] {
        ledger(&f, owner, run).await;
    }
    let prefix = match owner {
        Owner::Original => "migration_people_refresh",
        Owner::Admitted => "migration_admitted_people_refresh",
    };
    let versions: Vec<i64> = sqlx::query_scalar(&format!(
        "SELECT version FROM {prefix}_mapping_head WHERE organization_id=$1 ORDER BY version"
    ))
    .bind(f.org)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(versions, if hold_first { vec![1, 2] } else { vec![2, 2] });
    let (next, d) = fresh(&f, owner, parent, cohort, people).await;
    assert_eq!(
        d["plan"]["counts"]["held"],
        if hold_first { "1" } else { "0" }
    );
    assert_eq!(
        d["plan"]["counts"]["already_current"],
        if hold_first { "1" } else { "2" }
    );
    assert!(!next.is_nil());
}
async fn tick(f: &Fixture, owner: Owner) {
    match owner {
        Owner::Original => {
            ow::run_once(
                &f.pool,
                &f.key,
                &f.policy,
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
        }
        Owner::Admitted => {
            aw::run_once(
                &f.pool,
                &f.key,
                &f.policy,
                Some(&ReleaseReadiness::for_tests()),
            )
            .await
        }
    }
    .unwrap();
}
async fn fresh(
    f: &Fixture,
    owner: Owner,
    parent: Uuid,
    cohort: Uuid,
    people: Vec<Value>,
) -> (Uuid, Value) {
    match owner {
        Owner::Original => original::ready(f, parent, people).await,
        Owner::Admitted => {
            let report = admitted::report(f, parent, people).await;
            admitted::prepare(f, cohort, report).await
        }
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn original_second_repair_and_partial_cancel_preserve_exact_remainder(migrator: PgPool) {
    repeated_repair_and_cancelled_remainder(migrator, Owner::Original, false).await;
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn admitted_second_repair_and_partial_cancel_preserve_exact_remainder(migrator: PgPool) {
    repeated_repair_and_cancelled_remainder(migrator, Owner::Admitted, false).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn original_lost_identity_after_preview_holds_business_and_noop(migrator: PgPool) {
    let (f, parent, _) = original::mapped_fixture(&migrator).await;
    let mut people = original::rich_people();
    people[0]["firstName"] = json!("New name");
    let (run, d) = original::ready(&f, parent, people).await;
    original::confirm(&f, run, &original::confirmation(&d)).await;
    // Erasure/identity invalidation is injected with the migrator, never an app mutation.
    sqlx::query("ALTER TABLE migration_import_identity DISABLE TRIGGER USER")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query(
        "DELETE FROM migration_import_identity WHERE organization_id=$1 AND family='people'",
    )
    .bind(f.org)
    .execute(&migrator)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE migration_import_identity ENABLE TRIGGER USER")
        .execute(&migrator)
        .await
        .unwrap();
    until(&f, Owner::Original, run, "completed").await;
    let outcomes:Vec<String>=sqlx::query_scalar("SELECT disposition FROM migration_people_refresh_result WHERE refresh_id=$1 ORDER BY disposition").bind(run).fetch_all(&f.pool).await.unwrap();
    assert_eq!(outcomes, vec!["held_stale", "held_stale"]);
    let advanced: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM migration_people_refresh_baseline WHERE refresh_id=$1 AND result_id IS NOT NULL)",
    )
    .bind(run)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(!advanced);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn explicit_unassigned_noop_requires_its_counted_acknowledgement(migrator: PgPool) {
    let (f,parent,admission)=admitted::fixture_with_admission(&migrator,vec![json!({"id":104,"firstName":"Unassigned","stage":"Lead","assignedUserId":null,"assignedPondId":null})]).await;
    let report = admitted::report(
        &f,
        parent,
        vec![json!({"id":104,"firstName":"Unassigned","stage":"New stage","assignedUserId":999})],
    )
    .await;
    let (source, d) = admitted::prepare(&f, admission, report).await;
    let (run, _) = repair_preview(&f, Owner::Admitted, source, &d, f.lead_stage).await;
    let page = queries::mappings(
        &f.pool,
        &f.key,
        &f.ctx,
        Owner::Admitted,
        run,
        queries::Page::default(),
    )
    .await
    .unwrap();
    let assignee = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "assignee")
        .unwrap();
    commands::edit_choices(
        &f.pool,
        &f.key,
        &f.policy,
        &f.ctx,
        Owner::Admitted,
        run,
        commands::EditChoices {
            request_id: Uuid::new_v4(),
            expected_draft_revision: 1,
            choices: vec![commands::Choice {
                key_id: id(&assignee["id"]),
                disposition: "unassigned".into(),
                target_id: None,
            }],
        },
    )
    .await
    .unwrap();
    let d = seal(&f, Owner::Admitted, run).await;
    assert_eq!(d["plan"]["mapping_repair"]["unassigned_count"], "1");
    assert_eq!(d["plan"]["mapping_repair"]["approval_only_count"], "1");
    assert_eq!(d["plan"]["counts"]["assignment_clears"], "0");
    let mut body = original::confirmation(&d);
    body["acknowledged_eligible_count"] = json!(n(&d["plan"]["counts"]["eligible"]));
    body["mapping_repair"] = json!({"choices_digest":d["plan"]["mapping_repair"]["choices_digest"],"candidate_count":1,"approval_only_count":1,"unassigned_count":0});
    assert!(matches!(
        a::confirm(
            &f.pool,
            &f.key,
            &f.ctx,
            run,
            serde_json::from_value(body).unwrap(),
            Some(&ReleaseReadiness::for_tests())
        )
        .await,
        Err(MigrationError::Conflict)
    ));
    confirm(&f, Owner::Admitted, run, &d).await;
    until(&f, Owner::Admitted, run, "completed").await;
    ledger(&f, Owner::Admitted, run).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn repair_http_contract_enforces_tenant_role_and_strict_input(migrator: PgPool) {
    use crate::common::{body_json, get_with_cookie, post_json_with_cookie};
    let (f, parent, _) = original::mapped_fixture(&migrator).await;
    let mut people = original::rich_people();
    for person in &mut people {
        person["stage"] = json!("Needs mapping");
    }
    let (source, d) = original::ready(&f, parent, people).await;
    let path = format!("/api/migrations/fub/people-refreshes/{source}/mapping-repairs");
    let request = json!({"request_id":Uuid::new_v4(),"report_id":d["report_id"],"expected_lifecycle_revision":n(&d["lifecycle_revision"]),"anchor":{"kind":"preview","plan_id":d["plan"]["id"],"plan_revision":n(&d["plan"]["revision"])}});
    assert_eq!(
        post_json_with_cookie(&f.app, &path, &f.member_cookie, request.clone())
            .await
            .status(),
        403
    );
    let other = crate::import_support::fixture(
        &migrator,
        vec![json!({"id":900,"firstName":"Other org","stage":"Lead"})],
    )
    .await;
    assert_eq!(
        post_json_with_cookie(&other.app, &path, &other.cookie, request.clone())
            .await
            .status(),
        404
    );
    let mut invalid = request.clone();
    invalid["organization_id"] = json!(other.org);
    assert_eq!(
        post_json_with_cookie(&f.app, &path, &f.cookie, invalid)
            .await
            .status(),
        400
    );
    let response = post_json_with_cookie(&f.app, &path, &f.cookie, request.clone()).await;
    assert!(response.status().is_success());
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let saved = body_json(response).await;
    let run = id(&saved["refresh_id"]);
    assert_eq!(
        body_json(post_json_with_cookie(&f.app, &path, &f.cookie, request).await).await,
        saved
    );
    until(&f, Owner::Original, run, "paused").await;
    let mappings = format!("/api/migrations/fub/people-refreshes/{run}/repair-mappings");
    assert_eq!(
        get_with_cookie(&other.app, &mappings, &other.cookie)
            .await
            .status(),
        404
    );
    assert_eq!(
        get_with_cookie(&f.app, &mappings, &f.member_cookie)
            .await
            .status(),
        403
    );
    let page = body_json(get_with_cookie(&f.app, &mappings, &f.cookie).await).await;
    let stage = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "stage")
        .unwrap();
    let wrong = json!({"request_id":Uuid::new_v4(),"expected_draft_revision":0,"choices":[{"key_id":stage["id"],"disposition":"existing","target_id":other.lead_stage}]});
    assert!(!post_json_with_cookie(&f.app, &mappings, &f.cookie, wrong)
        .await
        .status()
        .is_success());
}

/// Opt-in real HTTP/browser fixture. Stop by creating OUTPUT/stop; the SQLx
/// database is isolated, source data synthetic, and production worker count unchanged.
#[sqlx::test]
#[ignore = "manual browser fixture; CRM_MAPPING_REPAIR_BROWSER_OUTPUT required"]
async fn mapping_repair_browser_fixture(migrator: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_MAPPING_REPAIR_BROWSER_OUTPUT")
            .expect("explicit browser fixture directory"),
    );
    assert!(output.is_absolute());
    std::fs::create_dir_all(&output).unwrap();
    let (f, parent, _) = original::mapped_fixture(&migrator).await;
    let mut people = original::rich_people();
    for person in &mut people {
        person["stage"] = json!("Imported VIP");
    }
    people[0]["firstName"] = json!("Updated from saved source");
    let (source, _) = original::ready(&f, parent, people).await;
    let email: String = sqlx::query_scalar("SELECT email FROM app_user WHERE id=$1")
        .bind(f.actor)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    std::fs::write(output.join("fixture.json"),serde_json::to_vec_pretty(&json!({"email":email,"password":"synthetic import fixture password","parent":parent,"refresh":source,"organization":f.org})).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3105")
        .await
        .unwrap();
    let router = f.app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    while !output.join("stop").exists() {
        tick(&f, Owner::Original).await;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    server.abort();
}

/// Inert cardinality fixture, never authority for a worker. The production
/// statement text is extracted verbatim; only fixed Owner table names expand.
#[sqlx::test]
#[ignore = "manual plan-shape slot; CRM_MAPPING_REPAIR_PLAN_OUTPUT required"]
async fn mapping_repair_25k_hot_plans(migrator: PgPool) {
    let output = std::path::PathBuf::from(
        std::env::var("CRM_MAPPING_REPAIR_PLAN_OUTPUT").expect("explicit plan output"),
    );
    assert!(output.is_absolute());
    let mut evidence = Vec::new();
    for owner in [Owner::Original, Owner::Admitted] {
        let (f, parent, cohort, mut people) = match owner {
            Owner::Original => {
                let (f, parent, _) = original::mapped_fixture(&migrator).await;
                (f, parent, parent, original::rich_people())
            }
            Owner::Admitted => {
                let people = vec![
                    json!({"id":104,"firstName":"Plan seed","stage":"Lead","assignedUserId":3}),
                ];
                let (f, parent, cohort) =
                    admitted::fixture_with_admission(&migrator, people.clone()).await;
                (f, parent, cohort, people)
            }
        };
        for v in &mut people {
            v["stage"] = json!("Plan hold");
        }
        let (source, d) = fresh(&f, owner, parent, cohort, people).await;
        let (run, _) = repair_preview(&f, owner, source, &d, f.lead_stage).await;
        let p = if owner == Owner::Original {
            "migration_people_refresh"
        } else {
            "migration_admitted_people_refresh"
        };
        // Opaque clones model relation volume only. No business write, worker,
        // or decryption runs against these rows. Restore the guard before EXPLAIN.
        sqlx::query(&format!(
            "ALTER TABLE {p}_repair_candidate DISABLE TRIGGER mapping_repair_owned"
        ))
        .execute(&migrator)
        .await
        .unwrap();
        sqlx::query(&format!("INSERT INTO {p}_item SELECT (jsonb_populate_record(NULL::{p}_item,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8004-'||lpad(g::text,12,'0'))::uuid,'source_key',(1000000+g)::text,'source_id',(1000000+g)::text,'disposition',CASE WHEN g%500=0 THEN 'held_mapping_gap' ELSE 'held_evidence_gap' END,'mapping_evidence_nonce',NULL,'mapping_evidence_ciphertext',NULL,'settled_result_id',NULL,'settled_at',NULL))).* FROM (SELECT * FROM {p}_item WHERE refresh_id=$1 LIMIT 1) seed CROSS JOIN generate_series(1,25000) g"))
            .bind(source).execute(&migrator).await.unwrap();
        sqlx::query(&format!("INSERT INTO {p}_repair_candidate SELECT (jsonb_populate_record(NULL::{p}_repair_candidate,to_jsonb(seed)||jsonb_build_object('anchor_item_id',('10000000-0000-4000-8004-'||lpad(g::text,12,'0'))::uuid,'source_id',(1000000+g)::text))).* FROM (SELECT * FROM {p}_repair_candidate WHERE refresh_id=$1 LIMIT 1) seed CROSS JOIN generate_series(1,25000) g"))
            .bind(run).execute(&migrator).await.unwrap();
        sqlx::query(&format!(
            "ALTER TABLE {p}_repair_candidate ENABLE TRIGGER mapping_repair_owned"
        ))
        .execute(&migrator)
        .await
        .unwrap();
        sqlx::query(&format!("INSERT INTO {p}_repair_key SELECT (jsonb_populate_record(NULL::{p}_repair_key,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8006-'||lpad(g::text,12,'0'))::uuid,'source_key_hmac',sha256(convert_to(g::text,'UTF8'))))).* FROM (SELECT * FROM {p}_repair_key WHERE refresh_id=$1 AND kind='stage' LIMIT 1) seed CROSS JOIN generate_series(1,25000) g")).bind(run).execute(&migrator).await.unwrap();
        sqlx::query(&format!("INSERT INTO {p}_repair_choice SELECT (jsonb_populate_record(NULL::{p}_repair_choice,to_jsonb(seed)||jsonb_build_object('id',('10000000-0000-4000-8007-'||lpad(g::text,12,'0'))::uuid,'revision',2,'source_key_hmac',sha256(convert_to(g::text,'UTF8'))))).* FROM (SELECT * FROM {p}_repair_choice WHERE refresh_id=$1 AND kind='stage' LIMIT 1) seed CROSS JOIN generate_series(1,25000) g")).bind(run).execute(&migrator).await.unwrap();
        for suffix in [
            "_item",
            "_result",
            "_repair_candidate",
            "_repair_key",
            "_repair_choice",
            "_mapping_head",
        ] {
            sqlx::query(&format!("ANALYZE {p}{suffix}"))
                .execute(&migrator)
                .await
                .unwrap();
        }
        let text = include_str!("../../crm-app/src/domain/migration/people_mapping_repair.rs");
        let marker = "\"WITH candidates AS";
        let at = text.find(marker).unwrap();
        let sql = serde_json::Deserializer::from_str(&text[at..].replace('\n', "\\n"))
            .into_iter::<String>()
            .next()
            .unwrap()
            .unwrap()
            .replace("{p}", p)
            .replace(
                "{success_column}",
                if owner == Owner::Original {
                    "original_result_id"
                } else {
                    "admission_result_id"
                },
            );
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
                .bind(source)
                .bind(id(&d["plan"]["id"]))
                .bind(f.org)
                .bind(None::<Uuid>)
                .bind("preview")
                .bind(false)
                .fetch_one(&migrator)
                .await
                .unwrap();
        assert!(
            plan.to_string().contains("Index"),
            "selective discovery uses an index: {plan}"
        );
        assert!(
            examined(&plan, &format!("{p}_item")) < 200.,
            "discovery must not walk 25k descriptors: {plan}"
        );
        evidence.push(json!({"owner":p,"statement":"discovery","sql":sql,"plan":plan}));
        // The exact materialized candidate-page query used by each existing worker.
        let worker = if owner == Owner::Original {
            include_str!("../../crm-app/src/domain/migration/people_refresh_worker.rs")
        } else {
            include_str!("../../crm-app/src/domain/migration/admitted_people_refresh_worker.rs")
        };
        let at = worker
            .find("\"WITH candidate_page AS MATERIALIZED")
            .unwrap();
        let sql = serde_json::Deserializer::from_str(&worker[at..].replace('\n', "\\n"))
            .into_iter::<String>()
            .next()
            .unwrap()
            .unwrap();
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
                .bind(cohort)
                .bind(f.org)
                .bind("")
                .bind(run)
                .fetch_one(&migrator)
                .await
                .unwrap();
        assert!(
            examined(&plan, &format!("{p}_repair_candidate")) <= 51.,
            "bounded candidate page: {plan}"
        );
        evidence.push(json!({"owner":p,"statement":"candidate_page","sql":sql,"plan":plan}));
        let statement = |body: &str, prefix: &str| {
            let at = body.find(&format!("\"{prefix}")).unwrap();
            serde_json::Deserializer::from_str(&body[at..].replace('\n', "\\n"))
                .into_iter::<String>()
                .next()
                .unwrap()
                .unwrap()
                .replace("{p}", p)
        };
        let q = include_str!("../../crm-app/src/domain/migration/people_mapping_repair_queries.rs");
        let hash: Vec<u8> = sqlx::query_scalar(&format!(
            "SELECT stage_source_hmac FROM {p}_repair_candidate WHERE refresh_id=$1 LIMIT 1"
        ))
        .bind(run)
        .fetch_one(&migrator)
        .await
        .unwrap();
        let sql = statement(q, "SELECT id FROM {p}_repair_key");
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
                .bind(run)
                .bind(f.org)
                .bind(None::<Uuid>)
                .bind(51_i64)
                .fetch_one(&migrator)
                .await
                .unwrap();
        assert!(examined(&plan, &format!("{p}_repair_key")) <= 51.);
        evidence.push(json!({"owner":p,"statement":"key_page","sql":sql,"plan":plan}));
        let sql = statement(text, "SELECT * FROM {p}_repair_choice WHERE refresh_id");
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
                .bind(run)
                .bind(f.org)
                .bind("stage")
                .bind(&hash)
                .bind(1_i64)
                .fetch_one(&migrator)
                .await
                .unwrap();
        assert!(examined(&plan, &format!("{p}_repair_choice")) <= 1.);
        evidence.push(json!({"owner":p,"statement":"frozen_choice","sql":sql,"plan":plan}));
        let sql = statement(q, "SELECT count(*) FROM {p}_repair_candidate")
            .replace("{column}", "stage_source_hmac");
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
                .bind(run)
                .bind(f.org)
                .bind(&hash)
                .fetch_one(&migrator)
                .await
                .unwrap();
        assert!(examined(&plan, &format!("{p}_repair_candidate")) <= 25002.);
        evidence.push(json!({"owner":p,"statement":"affected_count_linear","sql":sql,"plan":plan}));
        let sql = statement(text, "SELECT COALESCE((SELECT sum(octet_length(nonce)");
        let plan: Value =
            sqlx::query_scalar(&format!("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {sql}"))
                .bind(run)
                .bind(f.org)
                .fetch_one(&migrator)
                .await
                .unwrap();
        for suffix in ["_repair_key", "_repair_choice", "_repair_candidate"] {
            assert!(examined(&plan, &format!("{p}{suffix}")) <= 25002.);
        }
        evidence.push(json!({"owner":p,"statement":"retained_bytes_linear","sql":sql,"plan":plan}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}
fn examined(value: &Value, table: &str) -> f64 {
    match value {
        Value::Object(values) => {
            let own = if value["Relation Name"] == table {
                (value["Actual Rows"].as_f64().unwrap_or(0.)
                    + value["Rows Removed by Filter"].as_f64().unwrap_or(0.))
                    * value["Actual Loops"].as_f64().unwrap_or(1.)
            } else {
                0.
            };
            own + values.values().map(|v| examined(v, table)).sum::<f64>()
        }
        Value::Array(values) => values.iter().map(|v| examined(v, table)).sum(),
        _ => 0.,
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL migrator"]
async fn cancelled_repair_remainder_excludes_settled_holds_in_both_cohorts(migrator: PgPool) {
    repeated_repair_and_cancelled_remainder(migrator.clone(), Owner::Original, true).await;
    repeated_repair_and_cancelled_remainder(migrator, Owner::Admitted, true).await;
}

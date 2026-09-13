//! Coordinator acceptance for artifact capability, schema and scope boundaries.
use chrono::{Duration, Utc};
use crm_api::{
    auth::workspace::{self, ReleaseReadiness},
    domain::migration::{people_admission as admission, MigrationError},
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::{
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
};
use uuid::Uuid;

struct PrivateReport(PathBuf);
impl PrivateReport {
    fn new(value: &Value) -> Self {
        let dir = std::env::temp_dir().join(format!("crm-010e3-release-test-{}", Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let report = Self(dir.join("report.json"));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&report.0)
            .unwrap();
        f.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
        report
    }
}
impl Drop for PrivateReport {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}
async fn evidence(pool: &PgPool) -> Value {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await
        .unwrap();
    json!({"confirmation_ready":true,"people_admission_confirmation_ready":true,"database_name":database,"checked_at":Utc::now()-Duration::seconds(1),"evidence_expires_at":Utc::now()+Duration::minutes(4),"candidates":[{"sha256":workspace::artifact_fingerprint().await.unwrap(),"gate_version":workspace::GATE_VERSION,"role":"api","capabilities":[workspace::PEOPLE_ADMISSION_CAPABILITY]}]})
}
async fn load(pool: &PgPool, value: &Value) -> Result<ReleaseReadiness, sqlx::Error> {
    let file = PrivateReport::new(value);
    ReleaseReadiness::load_report(pool, &file.0).await
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn admission_real_release_partial_schema_and_unknown_engine_fail_closed(migrator: PgPool) {
    let f =
        crate::import_support::fixture(&migrator, crate::import_support::default_people()).await;
    let parent = crate::db_activity_source::completed_parent(&f).await;
    let id = crate::db_people_admission_execution::ready(
        &f,
        parent,
        vec![json!({"id":106,"firstName":"Release Probe","stage":"Lead","assignedUserId":3})],
    )
    .await;
    let base = evidence(&f.pool).await;
    let valid = load(&f.pool, &base).await.unwrap();
    assert!(valid.people_admission_ready());
    valid
        .require_people_admission(&mut f.pool.acquire().await.unwrap())
        .await
        .unwrap();
    let detail = admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    for case in [
        "missing capability",
        "false flag",
        "CLI role",
        "other artifact capability",
        "wrong database",
        "wrong hash",
        "expired",
        "wrong gate",
    ] {
        let mut value = base.clone();
        match case {
            "missing capability" => value["candidates"][0]["capabilities"] = json!([]),
            "false flag" => value["people_admission_confirmation_ready"] = json!(false),
            "CLI role" => value["candidates"][0]["role"] = json!("cli"),
            "other artifact capability" => {
                let mut other = value["candidates"][0].clone();
                other["sha256"] = json!("0".repeat(64));
                value["candidates"][0]["capabilities"] = json!([]);
                value["candidates"].as_array_mut().unwrap().push(other);
            }
            "wrong database" => value["database_name"] = json!("different_synthetic_database"),
            "wrong hash" => value["candidates"][0]["sha256"] = json!("0".repeat(64)),
            "expired" => value["evidence_expires_at"] = json!(Utc::now() - Duration::minutes(1)),
            "wrong gate" => value["candidates"][0]["gate_version"] = json!("unsupported-synthetic"),
            _ => unreachable!(),
        }
        let candidate = load(&f.pool, &value).await.ok();
        assert!(
            !candidate
                .as_ref()
                .is_some_and(ReleaseReadiness::people_admission_ready),
            "{case}"
        );
        let p = &detail["plan"];
        let cmd = admission::ConfirmPeopleAdmission {
            request_id: Uuid::new_v4(),
            plan_id: Uuid::parse_str(p["id"].as_str().unwrap()).unwrap(),
            plan_revision: p["revision"].as_str().unwrap().parse().unwrap(),
            plan_digest: p["digest"].as_str().unwrap().to_owned(),
            eligible_count: p["counts"]["eligible"].as_str().unwrap().parse().unwrap(),
            acknowledged_coverage: true,
            acknowledged_mappings: true,
            acknowledged_distinct_contacts: true,
            acknowledged_review_hold: true,
        };
        assert!(
            matches!(
                admission::confirm(&f.pool, &f.key, &f.ctx, id, cmd, candidate.as_ref()).await,
                Err(MigrationError::ReleaseNotReady)
            ),
            "{case}"
        );
        assert_eq!(
            admission::detail(&f.pool, &f.key, &f.ctx, id)
                .await
                .unwrap(),
            detail,
            "{case}: no receipt or state change"
        );
    }
    for mutation in ["ALTER TABLE migration_people_admission_contact RENAME TO synthetic_missing_admission_contact", "ALTER TABLE migration_import_identity RENAME COLUMN admission_result_id TO synthetic_missing_result_id"] {
        let mut tx=migrator.begin().await.unwrap();
        sqlx::query(mutation).execute(&mut *tx).await.unwrap();
        assert!(workspace::startup_compatible(&mut tx).await.is_err());
        assert!(valid.require_people_admission(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
    }
    let mut tx = migrator.begin().await.unwrap();
    sqlx::query("ALTER TABLE migration_people_admission DROP CONSTRAINT migration_people_admission_engine_version_check").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE migration_people_admission SET engine_version='unsupported-synthetic-engine' WHERE id=$1").bind(id).execute(&mut *tx).await.unwrap();
    assert!(workspace::startup_compatible(&mut tx).await.is_err());
    assert!(valid.require_people_admission(&mut tx).await.is_err());
    tx.rollback().await.unwrap();
    valid
        .require_people_admission(&mut f.pool.acquire().await.unwrap())
        .await
        .unwrap();
    let counts=sqlx::query("SELECT (SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1) results,(SELECT count(*) FROM migration_import_identity WHERE admission_id=$1) identities").bind(id).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts.get::<i64, _>("results"), 0);
    assert_eq!(counts.get::<i64, _>("identities"), 0);
}

fn confirmation(detail: &Value) -> admission::ConfirmPeopleAdmission {
    let p = &detail["plan"];
    admission::ConfirmPeopleAdmission {
        request_id: Uuid::new_v4(),
        plan_id: Uuid::parse_str(p["id"].as_str().unwrap()).unwrap(),
        plan_revision: p["revision"].as_str().unwrap().parse().unwrap(),
        plan_digest: p["digest"].as_str().unwrap().to_owned(),
        eligible_count: p["counts"]["eligible"].as_str().unwrap().parse().unwrap(),
        acknowledged_coverage: true,
        acknowledged_mappings: true,
        acknowledged_distinct_contacts: true,
        acknowledged_review_hold: true,
    }
}
#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn admission_cursor_pages_and_full_utf8_provenance_do_not_truncate(migrator: PgPool) {
    use crm_api::domain::migration::people_admission_worker;
    let f =
        crate::import_support::fixture(&migrator, crate::import_support::default_people()).await;
    let parent = crate::db_activity_source::completed_parent(&f).await;
    let long = "https://source.invalid/住所/".to_owned() + &"家🏠".repeat(9000);
    let emails = (0..56)
        .map(|n| json!({"value":format!("synthetic{n}@example.invalid")}))
        .collect::<Vec<_>>();
    let id=crate::db_people_admission_execution::ready(&f,parent,vec![json!({"id":106,"firstName":"Synthetic Unicode 家","stage":"Lead","assignedUserId":3,"emails":emails,"sourceUrl":long}),json!({"id":107,"firstName":"Separate Person","stage":"Lead","assignedUserId":3})]).await;
    let first = admission::items(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        admission::Page {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = admission::items(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        admission::Page {
            limit: Some(1),
            cursor: Some(cursor.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    assert!(second["next_cursor"].is_null());
    assert!(matches!(
        admission::items(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            admission::Page {
                limit: Some(2),
                cursor: Some(cursor),
                ..Default::default()
            }
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    let (selected, other) = if first["items"][0]["source_id"] == "106" {
        (&first["items"][0], &second["items"][0])
    } else {
        (&second["items"][0], &first["items"][0])
    };
    let item = Uuid::parse_str(selected["id"].as_str().unwrap()).unwrap();
    let other = Uuid::parse_str(other["id"].as_str().unwrap()).unwrap();
    let a = admission::contacts(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        item,
        admission::Page {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(a["contacts"].as_array().unwrap().len(), 50);
    let cursor = a["next_cursor"].as_str().unwrap().to_owned();
    assert!(matches!(
        admission::contacts(
            &f.pool,
            &f.key,
            &f.ctx,
            id,
            other,
            admission::Page {
                limit: Some(50),
                cursor: Some(cursor.clone()),
                ..Default::default()
            }
        )
        .await,
        Err(MigrationError::InvalidInput)
    ));
    let b = admission::contacts(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        item,
        admission::Page {
            limit: Some(50),
            cursor: Some(cursor),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(b["contacts"].as_array().unwrap().len(), 6);
    assert!(b["next_cursor"].is_null());
    let orders = a["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .chain(b["contacts"].as_array().unwrap())
        .map(|v| {
            v["import_order"]
                .as_str()
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(orders, (0..56).collect::<Vec<_>>());
    let detail = admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        confirmation(&detail),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    for _ in 0..10 {
        if !people_admission_worker::run_once(
            &f.pool,
            &f.key,
            &f.policy,
            Some(&ReleaseReadiness::for_tests()),
        )
        .await
        .unwrap()
        {
            break;
        }
    }
    assert_eq!(
        admission::detail(&f.pool, &f.key, &f.ctx, id)
            .await
            .unwrap()["state"],
        "completed"
    );
    let person:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND source_id='106'").bind(id).fetch_one(&f.pool).await.unwrap();
    let summary = admission::admission_provenance(&f.pool, &f.key, &f.ctx, person)
        .await
        .unwrap();
    assert!(serde_json::to_vec(&summary).unwrap().len() <= 128 * 1024);
    let mut complete = String::new();
    let mut cursor = None;
    for _ in 0..10 {
        let page = admission::admission_provenance_field(
            &f.pool,
            &f.key,
            &f.ctx,
            person,
            "sourceUrl".into(),
            admission::Page {
                cursor,
                limit: Some(16384),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            page["offset"].as_str().unwrap().parse::<usize>().unwrap(),
            complete.len()
        );
        complete.push_str(page["fragment"].as_str().unwrap());
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert!(cursor.is_none());
    assert_eq!(serde_json::from_str::<String>(&complete).unwrap(), long);
    let a = admission::results(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        admission::Page {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let b = admission::results(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        admission::Page {
            limit: Some(1),
            cursor: Some(a["next_cursor"].as_str().unwrap().into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_ne!(a["results"][0]["id"], b["results"][0]["id"]);
    assert!(b["next_cursor"].is_null());
}

#[sqlx::test]
#[ignore = "requires PostgreSQL migrator"]
async fn admission_permit_cannot_update_existing_people_or_borrow_other_lane_tokens(
    migrator: PgPool,
) {
    use crm_api::domain::migration::people_admission_worker;
    let f =
        crate::import_support::fixture(&migrator, crate::import_support::default_people()).await;
    let parent = crate::db_activity_source::completed_parent(&f).await;
    let id = crate::db_people_admission_execution::ready(
        &f,
        parent,
        vec![
            json!({"id":106,"firstName":"First New","stage":"Lead","assignedUserId":3}),
            json!({"id":107,"firstName":"Second New","stage":"Lead","assignedUserId":3}),
        ],
    )
    .await;
    let detail = admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    admission::confirm(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        confirmation(&detail),
        Some(&ReleaseReadiness::for_tests()),
    )
    .await
    .unwrap();
    assert!(people_admission_worker::run_once(
        &f.pool,
        &f.key,
        &f.policy,
        Some(&ReleaseReadiness::for_tests())
    )
    .await
    .unwrap());
    let row=sqlx::query("SELECT a.lease_token,i.id item_id,i.prospective_person_id FROM migration_people_admission a JOIN migration_people_admission_item i ON i.admission_id=a.id AND i.organization_id=a.organization_id WHERE a.id=$1 AND i.disposition='eligible' ORDER BY i.id LIMIT 1")
        .bind(id).fetch_one(&f.pool).await.unwrap();
    let lease: Uuid = row.get("lease_token");
    let item: Uuid = row.get("item_id");
    let target: Uuid = row.get("prospective_person_id");
    let existing:Uuid=sqlx::query_scalar("SELECT person_id FROM migration_people_admission_result WHERE admission_id=$1 AND disposition='settled'").bind(id).fetch_one(&f.pool).await.unwrap();
    let permit = json!({"lease":lease,"item":item}).to_string();
    for setting in [
        "crm.import_token",
        "crm.people_refresh_permit",
        "crm.people_admission_permit",
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config($1,$2,true)")
            .bind(setting)
            .bind(if setting == "crm.import_token" {
                lease.to_string()
            } else {
                permit.clone()
            })
            .execute(&mut *tx)
            .await
            .unwrap();
        let result=sqlx::query("INSERT INTO person(id,organization_id,first_name,stage_id) VALUES($1,$2,'Synthetic Scoped Permit',$3)").bind(target).bind(f.org).bind(f.lead_stage).execute(&mut *tx).await;
        assert_eq!(
            result.is_ok(),
            setting == "crm.people_admission_permit",
            "{setting}"
        );
        tx.rollback().await.unwrap();
    }
    for sql in [
        "UPDATE person SET first_name='Forbidden Rewrite' WHERE id=$1",
        "DELETE FROM person WHERE id=$1",
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.people_admission_permit',$1,true)")
            .bind(&permit)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(sqlx::query(sql)
            .bind(existing)
            .execute(&mut *tx)
            .await
            .is_err());
        tx.rollback().await.unwrap();
    }
    for (lease, item) in [(Uuid::new_v4(), item), (lease, Uuid::new_v4())] {
        let mut tx = f.pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('crm.people_admission_permit',$1,true)")
            .bind(json!({"lease":lease,"item":item}).to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(sqlx::query("INSERT INTO person(id,organization_id,first_name,stage_id) VALUES($1,$2,'Stale Permit',$3)").bind(target).bind(f.org).bind(f.lead_stage).execute(&mut *tx).await.is_err());
        tx.rollback().await.unwrap();
    }
    let detail = admission::detail(&f.pool, &f.key, &f.ctx, id)
        .await
        .unwrap();
    admission::cancel(
        &f.pool,
        &f.key,
        &f.ctx,
        id,
        admission::LifecyclePeopleAdmission {
            request_id: Uuid::new_v4(),
            expected_lifecycle_revision: detail["lifecycle_revision"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        },
    )
    .await
    .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('crm.people_admission_permit',$1,true)")
        .bind(&permit)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("INSERT INTO person(id,organization_id,first_name,stage_id) VALUES($1,$2,'Cancelled Permit',$3)").bind(target).bind(f.org).bind(f.lead_stage).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM migration_people_admission_result WHERE admission_id=$1"
        )
        .bind(id)
        .fetch_one(&f.pool)
        .await
        .unwrap(),
        1
    );
}

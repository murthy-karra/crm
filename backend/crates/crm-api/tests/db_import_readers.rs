//! Contact presentation parity on ordinary and imported order, including the
//! zero-Inquiry task-only path. Synthetic SQL rows are fixture setup only.
use crate::import_support::{default_people, fixture};
use chrono::Utc;
use crm_api::{
    domain::{
        person::{queries, visibility::PersonVisibilityScope},
        today,
    },
    ids::{OrganizationId, PersonId, UserId},
};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;
fn statement(function: &str) -> &'static str {
    let code = include_str!("../../crm-app/src/domain/person/queries.rs");
    code.split(&format!("pub async fn {function}("))
        .nth(1)
        .unwrap()
        .split("r#\"")
        .nth(1)
        .unwrap()
        .split("\"#,")
        .next()
        .unwrap()
}
#[sqlx::test]
#[ignore]
async fn imported_contact_order_is_consistent_for_direct_readers_and_task_only(migrator: PgPool) {
    let f = fixture(&migrator, default_people()).await;
    let org = OrganizationId::new(f.org);
    let scope = PersonVisibilityScope::Organization(org);
    let imported = Uuid::new_v4();
    let ordinary = Uuid::new_v4();
    for (id, name) in [
        (imported, "Synthetic ordered import"),
        (ordinary, "Synthetic ordinary"),
    ] {
        sqlx::query("INSERT INTO person(id,organization_id,first_name,stage_id,assigned_user_id) VALUES($1,$2,$3,$4,$5)").bind(id).bind(f.org).bind(name).bind(f.lead_stage).bind(f.actor).execute(&migrator).await.unwrap();
        for (kind, values) in [
            (
                "email",
                [
                    "old@synthetic.test",
                    "chosen@synthetic.test",
                    "later@synthetic.test",
                ],
            ),
            ("phone", ["4155550100", "4155550101", "4155550102"]),
        ] {
            for (n, value) in values.iter().enumerate() {
                let order = if id == imported {
                    Some([2, 0, 1][n])
                } else {
                    None
                };
                sqlx::query("INSERT INTO contact_method(id,organization_id,person_id,kind,value,normalized_value,created_at,import_order) VALUES($1,$2,$3,$4,$5,$5,now()+$6*interval '1 second',$7)").bind(Uuid::new_v4()).bind(f.org).bind(id).bind(kind).bind(value).bind(n as f64).bind(order).execute(&migrator).await.unwrap();
            }
        }
    }
    let rows = queries::list_summaries(&mut f.pool.acquire().await.unwrap(), &scope)
        .await
        .unwrap()
        .0;
    for row in &rows {
        let expected = if row.id.0 == imported {
            "chosen@synthetic.test"
        } else {
            "old@synthetic.test"
        };
        assert_eq!(row.primary_email.as_deref(), Some(expected));
    }
    let rows = queries::search_summaries(
        &mut f.pool.acquire().await.unwrap(),
        &scope,
        "Synthetic",
        50,
    )
    .await
    .unwrap()
    .0;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|r| r.id.0 == imported)
            .unwrap()
            .primary_phone
            .as_deref(),
        Some("4155550101")
    );
    let by_id = queries::summary_by_id(
        &mut f.pool.acquire().await.unwrap(),
        org,
        PersonId::new(imported),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        by_id.primary_email.as_deref(),
        Some("chosen@synthetic.test")
    );
    let contacts = queries::contact_methods_for_person(
        &mut f.pool.acquire().await.unwrap(),
        org,
        PersonId::new(imported),
    )
    .await
    .unwrap();
    let contacts = serde_json::to_value(contacts).unwrap();
    let email = contacts
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["kind"] == "email")
        .map(|v| v["value"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        email,
        vec![
            "chosen@synthetic.test",
            "later@synthetic.test",
            "old@synthetic.test"
        ]
    );
    sqlx::query("INSERT INTO task(id,organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,origin,correlation_id) VALUES($1,$2,$3,'Synthetic due task','other',now(),$4,$4,'web_session',gen_random_uuid())").bind(Uuid::new_v4()).bind(f.org).bind(imported).bind(f.actor).execute(&migrator).await.unwrap();
    let today = today::query_owned_at(
        f.pool.acquire().await.unwrap(),
        &scope,
        UserId::new(f.actor),
        Utc::now(),
    )
    .await
    .unwrap();
    let today = serde_json::to_value(today).unwrap();
    let item = today["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["person"]["id"] == imported.to_string())
        .expect("task-only Person is retained");
    assert_eq!(item["person"]["primary_email"], "chosen@synthetic.test");
    assert!(item["latest_inquiry"].is_null());
    // Actual hot SQL, with no planner switches, on 25k unrelated People and
    // four ordinary contacts per Person, including a dense shared phone. The
    // changed correlated contact lookups must remain indexed and locally bounded.
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id) SELECT $1,'Synthetic plan filler',$2 FROM generate_series(1,25000)").bind(f.org).bind(f.lead_stage).execute(&migrator).await.unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,created_at) SELECT p.organization_id,p.id,c.kind,c.value,c.value,p.created_at+c.n*interval '1 second' FROM person p CROSS JOIN (VALUES ('email','first@synthetic.test',0),('email','second@synthetic.test',1),('phone','4155550199',0),('phone','4155550198',1)) c(kind,value,n) WHERE p.organization_id=$1 AND p.first_name='Synthetic plan filler'").bind(f.org).execute(&migrator).await.unwrap();
    // A one-task relation permits equally cheap indexes, including the mobile
    // Person-page index. Future tasks for the same Organization and assignee
    // make the due-time predicate selective without changing the due result.
    sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,origin,correlation_id) SELECT p.organization_id,p.id,'Synthetic future task','other',now()+interval '30 days',$2,$2,'web_session',gen_random_uuid() FROM person p WHERE p.organization_id=$1 AND p.first_name='Synthetic plan filler'").bind(f.org).bind(f.actor).execute(&migrator).await.unwrap();
    sqlx::query("ANALYZE person")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("ANALYZE contact_method")
        .execute(&migrator)
        .await
        .unwrap();
    sqlx::query("ANALYZE task")
        .execute(&migrator)
        .await
        .unwrap();
    for function in ["list_summaries", "summary_by_id"] {
        let sql = format!(
            "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
            statement(function)
        );
        let mut q = sqlx::query(&sql).bind(f.org);
        if function == "summary_by_id" {
            q = q.bind(imported)
        }
        let row = q.fetch_one(&migrator).await.unwrap();
        let plan: Value = row.get(0);
        println!("CRM_010C_READER_PLAN {} {}", function, plan);
        assert_indexed_contacts(&plan);
    }
    let sql = format!(
        "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) {}",
        include_str!("../../crm-app/src/domain/today/sql/task_only.sql")
    );
    let row = sqlx::query(&sql)
        .bind(f.org)
        .bind(f.actor)
        .bind(Utc::now())
        .bind(Vec::<Uuid>::new())
        .bind(201i64)
        .fetch_one(&migrator)
        .await
        .unwrap();
    let plan: Value = row.get(0);
    println!("CRM_010C_READER_PLAN task_only {}", plan);
    assert!(plan.to_string().contains("task_org_assignee_due_open_idx"));
    assert_indexed_contacts(&plan);
}

fn assert_indexed_contacts(plan: &Value) {
    fn visit(node: &Value, count: &mut usize) {
        match node {
            Value::Object(object) => {
                if object.get("Relation Name").and_then(Value::as_str) == Some("contact_method") {
                    *count += 1;
                    let kind = object.get("Node Type").and_then(Value::as_str).unwrap();
                    assert!(
                        kind.contains("Index") || kind == "Bitmap Heap Scan",
                        "contact lookup must use an index: {node}"
                    );
                    let rows = object
                        .get("Actual Rows")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    assert!(
                        rows <= 3.0,
                        "contact lookup must stay scoped to one Person/kind: {node}"
                    );
                }
                for value in object.values() {
                    visit(value, count);
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, count);
                }
            }
            _ => {}
        }
    }
    let mut count = 0;
    visit(plan, &mut count);
    assert!(count >= 2, "both primary contact lookups must be present");
}

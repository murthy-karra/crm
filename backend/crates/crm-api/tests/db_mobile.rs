//! Mobile001 real PostgreSQL/router contract and failure-path proof.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::Timelike;
use crm_api::{
    auth::AuthContext,
    domain::{
        admin::{MembershipStatus, Role},
        envelope::CommandContext,
        mobile, note, task,
    },
    ids::{NoteId, OrganizationId, PersonId, TaskId, UserId},
    realtime::Publisher,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
const PW: &str = "mobile-fixture-password-12345";
struct Fixture {
    org: Uuid,
    actor: Uuid,
    other: Uuid,
    person: Uuid,
    app: PgPool,
    router: Router,
    cookie: String,
    context: Uuid,
    install: Uuid,
    bootstrap: Value,
}

#[sqlx::test]
#[ignore]
async fn mobile006_metadata_atomic_receipt_current_and_catalog_generation(pool: PgPool) {
    let f = fixture(&pool).await;
    let tag: Uuid = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 tag') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let field: Uuid = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) VALUES($1,'Mobile006 budget','number',1,$2) RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let (metadata, catalog):(i64,i64)=sqlx::query_as("SELECT p.metadata_revision,c.revision FROM person p JOIN mobile_metadata_catalog c ON c.organization_id=p.organization_id WHERE p.id=$1")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    let operation = f.operation("update_person_metadata", json!({"person_id":f.person,"expected_metadata_revision":metadata.to_string(),"expected_catalog_revision":catalog.to_string(),"actions":[{"kind":"add_tag","tag_id":tag},{"kind":"set_field","field_id":field,"value":{"number":"123.4500"}}]}));
    let (status, accepted) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["resource_type"], "person_metadata");
    assert_eq!(accepted["committed_revision"], (metadata + 2).to_string());
    let (_, replay) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(replay["replayed"], true);
    let current_path = format!("/api/mobile/v1/people/{}/metadata", f.person);
    let (status, current) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &current_path,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["tags"][0]["id"], json!(tag));
    assert_eq!(current["values"][0]["value"]["number"], "123.4500");
    // A catalog-only change conflicts before a fresh mutation, and an invalid
    // sibling rolls the whole atomic patch back.
    let next_metadata: i64 = current["metadata_revision"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let stale_catalog: i64 = current["catalog_revision"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let rollback_tag: Uuid = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 rollback') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let stale = f.operation("update_person_metadata", json!({"person_id":f.person,"expected_metadata_revision":next_metadata.to_string(),"expected_catalog_revision":stale_catalog.to_string(),"actions":[{"kind":"add_tag","tag_id":rollback_tag}]}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", stale).await.1["error"],
        "catalog_revision_conflict"
    );
    let catalog: i64 =
        sqlx::query_scalar("SELECT revision FROM mobile_metadata_catalog WHERE organization_id=$1")
            .bind(f.org)
            .fetch_one(&f.app)
            .await
            .unwrap();
    let invalid = f.operation("update_person_metadata", json!({"person_id":f.person,"expected_metadata_revision":next_metadata.to_string(),"expected_catalog_revision":catalog.to_string(),"actions":[{"kind":"add_tag","tag_id":rollback_tag},{"kind":"set_field","field_id":field,"value":{"number":"1e4"}}]}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", invalid).await.1["error"],
        "invalid_metadata"
    );
    let links: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM person_tag WHERE organization_id=$1 AND person_id=$2 AND tag_id=$3",
    )
    .bind(f.org)
    .bind(f.person)
    .bind(rollback_tag)
    .fetch_one(&f.app)
    .await
    .unwrap();
    assert_eq!(links, 0);
    let (status, generation)=f.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[f.person],"include_metadata":true})).await;
    assert_eq!(status, StatusCode::OK, "{generation}");
    let generation_id = id(&generation, "generation_id");
    let path = format!("/api/mobile/v1/reconciliations/{generation_id}/metadata/catalog/fields");
    let (status, fields) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &path,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fields}");
    assert!(fields["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == json!(field)));
    let component_path = format!(
        "/api/mobile/v1/reconciliations/{generation_id}/people/{}/metadata",
        f.person
    );
    let (status, component) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &component_path,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{component}");
    assert_eq!(component["section"], "metadata");
    assert_eq!(component["metadata_revision"], current["metadata_revision"]);
    assert_eq!(component["catalog_revision"], json!(catalog.to_string()));
    assert_eq!(component["complete"], true);
}

#[sqlx::test]
#[ignore]
async fn mobile006_metadata_events_and_noop_replay_are_content_free(pool: PgPool) {
    let mut f = fixture(&pool).await;
    let publisher = Publisher::recording();
    f.router = crate::common::build_router_with_publisher(&pool, publisher.clone()).await;
    let tag: Uuid = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 event tag') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let field: Uuid = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) VALUES($1,'Mobile006 event value','text',1,$2) RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let (metadata, catalog): (i64, i64) = sqlx::query_as("SELECT p.metadata_revision,c.revision FROM person p JOIN mobile_metadata_catalog c ON c.organization_id=p.organization_id WHERE p.id=$1")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    let operation = f.operation(
        "update_person_metadata",
        json!({
            "person_id": f.person,
            "expected_metadata_revision": metadata.to_string(),
            "expected_catalog_revision": catalog.to_string(),
            "actions": [
                {"kind":"add_tag","tag_id":tag},
                {"kind":"set_field","field_id":field,"value":{"text":"Private field value"}}
            ]
        }),
    );
    let (status, accepted) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let Publisher::Recording(events, _) = &publisher else {
        unreachable!()
    };
    let publications = events.lock().await;
    assert_eq!(publications.len(), 2);
    let changes: Vec<_> = publications
        .iter()
        .map(|publication| publication.1["data"]["change"].clone())
        .collect();
    assert_eq!(
        changes,
        vec![json!("custom_field_changed"), json!("tags_changed")]
    );
    assert!(publications
        .iter()
        .all(|publication| !publication.1.to_string().contains("Private field value")));
    drop(publications);
    let (_, replay) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(replay["replayed"], true);
    assert_eq!(events.lock().await.len(), 2);
    let noop = f.operation(
        "update_person_metadata",
        json!({
            "person_id": f.person,
            "expected_metadata_revision": accepted["committed_revision"],
            "expected_catalog_revision": catalog.to_string(),
            "actions": [
                {"kind":"add_tag","tag_id":tag},
                {"kind":"set_field","field_id":field,"value":{"text":"Private field value"}}
            ]
        }),
    );
    let (status, unchanged) = f.post("/api/mobile/v1/operations", noop).await;
    assert_eq!(status, StatusCode::OK, "{unchanged}");
    assert_eq!(unchanged["changed"], false);
    assert_eq!(events.lock().await.len(), 2);
}

#[sqlx::test]
#[ignore]
async fn mobile006_metadata_catalog_change_invalidates_only_opted_generation(pool: PgPool) {
    let f = fixture(&pool).await;
    let legacy = f.gen().await;
    let legacy_id = id(&legacy, "generation_id");
    assert!(legacy.get("metadata_catalog").is_none());
    let (status, opted) = f
        .post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[f.person],"include_metadata":true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{opted}");
    let opted_id = id(&opted, "generation_id");
    assert_eq!(
        legacy["people"]["items"][0]["revision"],
        opted["people"]["items"][0]["revision"]
    );
    let metadata = format!(
        "/api/mobile/v1/reconciliations/{opted_id}/people/{}/metadata",
        f.person
    );
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &metadata,
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
    sqlx::query("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 catalog changed')")
        .bind(f.org)
        .bind(f.actor)
        .execute(&f.app)
        .await
        .unwrap();
    let (status, component) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &metadata,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{component}");
    assert_eq!(component["error"], "generation_changed");
    let (_, sealed) = f
        .post(
            &format!("/api/mobile/v1/reconciliations/{opted_id}/seal"),
            json!({}),
        )
        .await;
    assert_eq!(sealed["error"], "generation_changed");
    for section in ["summary", "notes", "tasks"] {
        let path = format!(
            "/api/mobile/v1/reconciliations/{legacy_id}/people/{}/{}",
            f.person, section
        );
        let (status, page) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &path,
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["complete"], true);
    }
    let (status, sealed) = f
        .post(
            &format!("/api/mobile/v1/reconciliations/{legacy_id}/seal"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sealed}");
}

#[sqlx::test]
#[ignore]
async fn mobile006_metadata_reader_establishes_snapshot_after_catalog_barrier(pool: PgPool) {
    let f = fixture(&pool).await;
    let tag: Uuid = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 snapshot tag') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let generation = f
        .post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[f.person],"include_metadata":true}),
        )
        .await
        .1;
    let generation_id = id(&generation, "generation_id");
    let component = format!(
        "/api/mobile/v1/reconciliations/{generation_id}/people/{}/metadata",
        f.person
    );
    let mut writer = f.app.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('crm-mobile-metadata-catalog:' || $1::text, 0))")
        .bind(f.org)
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE tag SET name='Mobile006 snapshot renamed' WHERE organization_id=$1 AND id=$2",
    )
    .bind(f.org)
    .bind(tag)
    .execute(&mut *writer)
    .await
    .unwrap();
    let reader = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &component,
        json!(null),
    );
    tokio::pin!(reader);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(75), &mut reader)
            .await
            .is_err()
    );
    writer.commit().await.unwrap();
    let (status, changed) = reader.await;
    assert_eq!(status, StatusCode::CONFLICT, "{changed}");
    assert_eq!(changed["error"], "generation_changed");
}

#[sqlx::test]
#[ignore]
async fn mobile006_metadata_receipt_and_current_authority_gates(pool: PgPool) {
    let f = fixture(&pool).await;
    let tag: Uuid = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 authority tag') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let (metadata, catalog): (i64, i64) = sqlx::query_as("SELECT p.metadata_revision,c.revision FROM person p JOIN mobile_metadata_catalog c ON c.organization_id=p.organization_id WHERE p.id=$1")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    let operation = f.operation("update_person_metadata", json!({"person_id":f.person,"expected_metadata_revision":metadata.to_string(),"expected_catalog_revision":catalog.to_string(),"actions":[{"kind":"add_tag","tag_id":tag}]}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", operation.clone())
            .await
            .0,
        StatusCode::OK
    );
    let mut altered = operation.clone();
    altered["payload"]["actions"][0]["kind"] = json!("remove_tag");
    assert_eq!(
        f.post("/api/mobile/v1/operations", altered).await.1["error"],
        "operation_payload_mismatch"
    );
    let other_cookie = crate::common::login_cookie(&f.router, "second@fixture.test", PW).await;
    let (status, other_bootstrap) = request(
        &f.router,
        &other_cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":Uuid::new_v4()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let receipt = format!(
        "/api/mobile/v1/operations/{}",
        operation["operation_id"].as_str().unwrap()
    );
    assert_eq!(
        request(
            &f.router,
            &other_cookie,
            Some(id(&other_bootstrap, "context_id")),
            "GET",
            &receipt,
            json!(null)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let current = format!("/api/mobile/v1/people/{}/metadata", f.person);
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &current,
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &current,
            json!(null)
        )
        .await
        .1["error"],
        "workspace_in_migration_review"
    );
}

#[sqlx::test]
#[ignore]
async fn mobile006_current_metadata_rejects_more_than_one_hundred_values(pool: PgPool) {
    let f = fixture(&pool).await;
    for n in 0..101 {
        let field: Uuid = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) VALUES($1,$2,'text',$3,$4) RETURNING id")
            .bind(f.org).bind(format!("Mobile006 bound {n}")).bind(n).bind(f.actor).fetch_one(&f.app).await.unwrap();
        sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,'text','bound',$4,'mobile_session',gen_random_uuid())")
            .bind(f.org).bind(f.person).bind(field).bind(f.actor).execute(&f.app).await.unwrap();
    }
    let path = format!("/api/mobile/v1/people/{}/metadata", f.person);
    let (status, error) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &path,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    assert_eq!(error["error"], "over_limit");
}

#[cfg(feature = "perf-harness")]
#[sqlx::test]
#[ignore = "coordinator-owned M6-09 isolated hot-plan run"]
async fn mobile006_metadata_hot_query_plans(pool: PgPool) {
    use sha2::{Digest, Sha256};

    let f = fixture(&pool).await;
    for n in 0..48 {
        let email = format!("mobile006-plan-member-{n}@fixture.test");
        let user = crate::common::create_user(&pool, &email, "Plan Member", PW).await;
        crate::common::add_membership_with(
            &pool,
            f.org,
            user,
            Role::Member,
            MembershipStatus::Active,
        )
        .await;
    }
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM organization_membership WHERE organization_id=$1 AND status='active'").bind(f.org).fetch_one(&f.app).await.unwrap(), 50);
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Mobile006 plan',p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,24999) WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    let _tag: Uuid = sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Mobile006 plan tag') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    let _field: Uuid = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) VALUES($1,'Mobile006 plan field','text',1,$2) RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,created_by_user_id) VALUES($1,$2,$3,$4)")
        .bind(f.org).bind(f.person).bind(_tag).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,'text','plan',$4,'mobile_session',gen_random_uuid())")
        .bind(f.org).bind(f.person).bind(_field).bind(f.actor).execute(&f.app).await.unwrap();
    let statements = [
        ("membership", "SELECT role FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND status='active' FOR SHARE"),
        ("person_token", "SELECT metadata_revision FROM person WHERE organization_id=$1 AND id=$2"),
        ("catalog_token", "SELECT crm_mobile_metadata_catalog_revision($1)"),
        ("tags", "SELECT t.id,t.name FROM person_tag pt JOIN tag t ON t.id=pt.tag_id AND t.organization_id=pt.organization_id WHERE pt.organization_id=$1 AND pt.person_id=$2 ORDER BY t.id LIMIT 101"),
        ("values", "SELECT field_id,field_type,text_value,number_value,date_value,option_id,updated_at FROM person_custom_field_value WHERE organization_id=$1 AND person_id=$2 ORDER BY field_id LIMIT 101"),
        ("catalog_tags", "SELECT id,name FROM tag WHERE organization_id=$1 ORDER BY id LIMIT 10001"),
        ("catalog_fields", "SELECT id,label,field_type,position,archived_at FROM custom_field WHERE organization_id=$1 ORDER BY position,id LIMIT 10001"),
        ("catalog_options", "SELECT id,field_id,label,position,archived_at FROM custom_field_option WHERE organization_id=$1 ORDER BY field_id,position,id LIMIT 10001"),
    ];
    let mut evidence = Vec::new();
    for (name, sql) in statements {
        let explain = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}");
        let plan: Value = match name {
            "membership" => {
                sqlx::query_scalar(&explain)
                    .bind(f.org)
                    .bind(f.actor)
                    .fetch_one(&f.app)
                    .await
            }
            "catalog_token" => {
                sqlx::query_scalar(&explain)
                    .bind(f.org)
                    .fetch_one(&f.app)
                    .await
            }
            _ => {
                sqlx::query_scalar(&explain)
                    .bind(f.org)
                    .bind(f.person)
                    .fetch_one(&f.app)
                    .await
            }
        }
        .unwrap();
        let sha256: String = Sha256::digest(sql.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        evidence.push(json!({"name":name,"sql":sql,"sha256":sha256,"plan":plan}));
    }
    println!(
        "MOBILE006_HOT {}",
        json!({"people":25000,"members":50,"plans":evidence})
    );
}

#[sqlx::test]
#[ignore]
async fn mobile005_details_receipt_replay_and_revision_scope(pool: PgPool) {
    let f = fixture(&pool).await;
    assert!(f.bootstrap["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "update_person_details"));
    let revision: i64 = sqlx::query_scalar("SELECT details_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let operation = f.operation("update_person_details", json!({
        "person_id":f.person,"expected_details_revision":revision.to_string(),"last_name":"  Example  ",
        "contact_operations":[
            {"op":"add","kind":"email","value":" details@example.test "},
            {"op":"add","kind":"phone","value":"(555) 555-0100"}
        ]
    }));
    let (status, accepted) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK, "accepted: {accepted}");
    assert_eq!(accepted["resource_type"], "person_details");
    assert_eq!(accepted["committed_revision"], (revision + 3).to_string());
    assert_eq!(accepted["added_contact_ids"][0]["ordinal"], 0);
    assert_eq!(accepted["added_contact_ids"][1]["ordinal"], 1);
    let (_, replay) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["added_contact_ids"], accepted["added_contact_ids"]);
    let details_url = format!("/api/mobile/v1/people/{}/details", f.person);
    let (details_status, details) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &details_url,
        json!(null),
    )
    .await;
    assert_eq!(details_status, StatusCode::OK, "details: {details}");
    assert_eq!(details["last_name"], "Example");
    assert_eq!(details["items"].as_array().unwrap().len(), 2);
    let email_id = id(&accepted["added_contact_ids"][0], "id");
    let phone_id = id(&accepted["added_contact_ids"][1], "id");
    let edit_remove = f.operation("update_person_details", json!({
        "person_id":f.person,"expected_details_revision":(revision + 3).to_string(),"contact_operations":[
            {"op":"edit","id":email_id,"value":"updated@example.test"},
            {"op":"remove","id":phone_id}
        ]
    }));
    let (edit_status, edited) = f.post("/api/mobile/v1/operations", edit_remove).await;
    assert_eq!(edit_status, StatusCode::OK, "edited: {edited}");
    assert_eq!(edited["committed_revision"], (revision + 5).to_string());
    // Stage-only changes do not advance the details aggregate.
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(f.org)
    .fetch_one(&f.app)
    .await
    .unwrap();
    sqlx::query("UPDATE person SET stage_id=$3 WHERE id=$1 AND organization_id=$2")
        .bind(f.person)
        .bind(f.org)
        .bind(stage)
        .execute(&f.app)
        .await
        .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT details_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert_eq!(after, revision + 5);
    // Parent erasure remains legal when the contact trigger runs from the
    // ON DELETE CASCADE path; no surviving parent is required.
    sqlx::query("DELETE FROM person WHERE id=$1 AND organization_id=$2")
        .bind(f.person)
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
}
fn record(name: &str, value: &Value) {
    if let Ok(directory) = std::env::var("MOBILE_FIXTURE_DIR") {
        let path = std::path::Path::new(&directory);
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(
            path.join(format!("{name}.json")),
            serde_json::to_string_pretty(value).unwrap() + "\n",
        )
        .unwrap();
    }
}
async fn request(
    router: &Router,
    cookie: &str,
    context: Option<Uuid>,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("cookie", cookie);
    if let Some(context) = context {
        builder = builder.header("x-mobile-context", context.to_string());
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn id(value: &Value, key: &str) -> Uuid {
    value[key].as_str().unwrap().parse().unwrap()
}
async fn fixture(pool: &PgPool) -> Fixture {
    let (org, actor) = crate::common::create_org_with_stages_and_member(
        pool,
        "Mobile Fixture",
        "mobile@fixture.test",
        "Mobile Agent",
        PW,
    )
    .await;
    let other = crate::common::create_user(pool, "second@fixture.test", "Other Agent", PW).await;
    crate::common::add_membership_with(pool, org, other, Role::Member, MembershipStatus::Active)
        .await;
    let app = crate::common::connect_as_app(pool).await;
    let stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(org)
    .fetch_one(&app)
    .await
    .unwrap();
    let person:Uuid=sqlx::query_scalar("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) VALUES($1,'Synthetic Person',$2,$3) RETURNING id").bind(org).bind(stage).bind(actor).fetch_one(&app).await.unwrap();
    let router = crate::common::build_router(pool).await;
    let cookie = crate::common::login_cookie(&router, "mobile@fixture.test", PW).await;
    let install = Uuid::new_v4();
    let (status, bootstrap) = request(
        &router,
        &cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":install}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bootstrap: {bootstrap}");
    let context = id(&bootstrap, "context_id");
    assert_eq!(bootstrap["actor_user_id"], actor.to_string());
    assert_eq!(bootstrap["organization_id"], org.to_string());
    let authorized: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(bootstrap["authorized_at"].clone()).unwrap();
    let expires: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(bootstrap["offline_access_expires_at"].clone()).unwrap();
    assert_eq!(expires - authorized, chrono::Duration::days(7));
    Fixture {
        org,
        actor,
        other,
        person,
        app,
        router,
        cookie,
        context,
        install,
        bootstrap,
    }
}
impl Fixture {
    fn auth(&self) -> AuthContext {
        AuthContext {
            actor_user_id: UserId(self.actor),
            actor_email: "mobile@fixture.test".into(),
            actor_display_name: "Mobile Agent".into(),
            active_organization_id: OrganizationId(self.org),
            active_organization_name: "Mobile Fixture".into(),
            role: Role::Member,
        }
    }
    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        request(
            &self.router,
            &self.cookie,
            Some(self.context),
            "POST",
            path,
            body,
        )
        .await
    }
    fn operation(&self, kind: &str, payload: Value) -> Value {
        json!({"context_id":self.context,"operation_id":Uuid::new_v4(),"kind":kind,"device_recorded_at":"2026-09-12T20:00:00Z","payload":payload})
    }
    async fn gen(&self) -> Value {
        let (status,value)=self.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":self.install,"pinned_person_ids":[]})).await;
        assert_eq!(status, StatusCode::OK, "generation: {value}");
        value
    }
}
#[sqlx::test]
#[ignore]
async fn atomic_replay_conflict_dependency_and_old_web_commands(pool: PgPool) {
    let f = fixture(&pool).await;
    let note = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"  Field note\r\nSaved  "}),
    );
    let (left, right) = tokio::join!(
        f.post("/api/mobile/v1/operations", note.clone()),
        f.post("/api/mobile/v1/operations", note.clone())
    );
    assert_eq!(left.0, StatusCode::OK, "left: {}", left.1);
    assert_eq!(right.0, StatusCode::OK, "right: {}", right.1);
    assert!(
        left.1.get("added_contact_ids").is_none(),
        "legacy receipt JSON stays unchanged"
    );
    assert!(right.1.get("added_contact_ids").is_none());
    assert_eq!(left.1["resource_id"], right.1["resource_id"]);
    assert_ne!(left.1["replayed"], right.1["replayed"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    let mut normalized = note.clone();
    normalized["payload"]["body"] = json!("Field note\nSaved");
    let (status, replay) = f.post("/api/mobile/v1/operations", normalized).await;
    record("operation_replay", &replay);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    let mut mismatch = note.clone();
    mismatch["payload"]["body"] = json!("Different proposal");
    let (status, error) = f.post("/api/mobile/v1/operations", mismatch).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"], "operation_payload_mismatch");
    record("payload_mismatch", &error);
    let create=f.operation("create_task",json!({"person_id":f.person,"title":"Visit after outage","kind":"follow_up","due_at":null,"assignee_user_id":null}));
    let complete = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"created_by_operation_id":create["operation_id"]}}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", complete.clone())
            .await
            .1["error"],
        "dependency_pending"
    );
    let (status, created) = f.post("/api/mobile/v1/operations", create.clone()).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["committed_revision"], "1");
    record("create_task_request", &create);
    record("create_task_receipt", &created);
    let (status, completed) = f.post("/api/mobile/v1/operations", complete.clone()).await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["committed_revision"], "2");
    record("complete_dependency_request", &complete);
    record("complete_task_receipt", &completed);
    let task_id = id(&created, "resource_id");
    task::reopen_task(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&f.auth()),
        task::ReopenTask {
            person_id: PersonId(f.person),
            task_id: TaskId(task_id),
        },
    )
    .await
    .unwrap();
    let (status, replay) = f.post("/api/mobile/v1/operations", complete).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    let still_open: bool = sqlx::query_scalar("SELECT completed_at IS NULL FROM task WHERE id=$1")
        .bind(task_id)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert!(still_open, "replay must not complete a reopened task");
    let stale = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"task_id":task_id,"expected_revision":"1"}}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", stale).await.1["error"],
        "revision_conflict"
    );
    // Database failure after business write but before receipt must roll back both.
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic receipt failure'; END $$; CREATE TRIGGER mobile_fixture_failure BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile_fixture_fail_receipt();").execute(&pool).await.unwrap();
    let failed = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"Must stay queued"}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", failed.clone()).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER mobile_fixture_failure ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", failed).await.0,
        StatusCode::OK
    );
    // Seven-day lock protects old queue identity; same-identity bootstrap renews.
    sqlx::query(
        "UPDATE mobile_context SET offline_access_expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(f.context)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", note.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (_, renewed) = request(
        &f.router,
        &f.cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":f.install}),
    )
    .await;
    assert_eq!(renewed["context_id"], f.context.to_string());
    record("bootstrap", &renewed);
    record("add_note_request", &note);
    assert_eq!(
        f.post("/api/mobile/v1/operations", note).await.1["replayed"],
        true
    );
    // Payloads never enter durable receipts; only keyed digests and result IDs.
    let cols:Vec<String>=sqlx::query_scalar("SELECT column_name FROM information_schema.columns WHERE table_name='mobile_operation_receipt'").fetch_all(&pool).await.unwrap();
    assert!(!cols
        .iter()
        .any(|v| matches!(v.as_str(), "body" | "title" | "payload" | "raw_request")));
}

/// Mobile 002's narrow edit contract: independent note versions, strict
/// stale detection before a no-op shortcut, preserved NULL assignees, and
/// bounded current-record reads.  It deliberately uses the real router and
/// app role to exercise receipt and workspace transactions together.
#[sqlx::test]
#[ignore]
async fn mobile002_edit_revisions_receipts_and_current_reads(pool: PgPool) {
    let f = fixture(&pool).await;
    assert!(f.bootstrap["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "edit_note"));
    assert!(f.bootstrap["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "update_task"));
    assert!(f.bootstrap["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "note_revisions"));

    let add = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"mobile baseline"}),
    );
    let (status, add_receipt) = f.post("/api/mobile/v1/operations", add).await;
    assert_eq!(status, StatusCode::OK, "{add_receipt}");
    assert!(add_receipt["committed_revision"].is_null());
    let note_id = id(&add_receipt, "resource_id");
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM note WHERE id=$1")
        .bind(note_id)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert_eq!(revision, 1);

    let edit = f.operation(
        "edit_note",
        json!({"person_id":f.person,"note_id":note_id,"expected_revision":"1","body":"  edited\r\nbody  "}),
    );
    let (status, edited) = f.post("/api/mobile/v1/operations", edit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["committed_revision"], "2");
    assert_eq!(edited["changed"], true);
    record("mobile002_edit_note_receipt", &edited);
    assert_eq!(
        f.post("/api/mobile/v1/operations", edit).await.1["replayed"],
        true
    );
    let stale_equal = f.operation(
        "edit_note",
        json!({"person_id":f.person,"note_id":note_id,"expected_revision":"1","body":"edited\nbody"}),
    );
    let (status, conflict) = f.post("/api/mobile/v1/operations", stale_equal).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"], "revision_conflict");
    assert!(conflict.get("body").is_none());

    // A non-body writer also advances the independent token, while a direct
    // identical update leaves it stable.
    sqlx::query("UPDATE note SET origin='migration' WHERE id=$1")
        .bind(note_id)
        .execute(&f.app)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM note WHERE id=$1")
            .bind(note_id)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        3
    );
    sqlx::query("UPDATE note SET origin='migration' WHERE id=$1")
        .bind(note_id)
        .execute(&f.app)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM note WHERE id=$1")
            .bind(note_id)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        3
    );

    let (status, current) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{}/notes/{note_id}", f.person),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["note"]["revision"], "3");
    assert_eq!(current["person_id"], f.person.to_string());
    record("mobile002_current_note", &current);
    let (status, malformed) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{}/notes/{note_id}?extra=1", f.person),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(malformed["error"], "malformed_request");
    let (status, bad_context) = request(
        &f.router,
        &f.cookie,
        Some(Uuid::new_v4()),
        "GET",
        &format!("/api/mobile/v1/people/{}/notes/{note_id}", f.person),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(bad_context["error"], "unauthenticated");
    let (status, rejected_body) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{}/notes/{note_id}", f.person),
        json!({"organization_id":Uuid::new_v4()}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(rejected_body["error"], "malformed_request");

    // Imported/legacy records can have no assignee. A mobile edit preserves
    // that exact NULL rather than assigning the editing actor.
    let task_id: Uuid = sqlx::query_scalar(
        "INSERT INTO task(organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,origin,correlation_id) \
         VALUES($1,$2,'legacy unassigned','follow_up',NULL,NULL,$3,'migration',$4) RETURNING id",
    )
    .bind(f.org)
    .bind(f.person)
    .bind(f.actor)
    .bind(Uuid::new_v4())
    .fetch_one(&f.app)
    .await
    .unwrap();
    let update = f.operation(
        "update_task",
        json!({"person_id":f.person,"task_id":task_id,"expected_revision":"1","title":"updated task","kind":"call","due_at":null}),
    );
    let omitted_due_at = f.operation(
        "update_task",
        json!({"person_id":f.person,"task_id":task_id,"expected_revision":"1","title":"updated task","kind":"call"}),
    );
    let (status, omitted_error) = f.post("/api/mobile/v1/operations", omitted_due_at).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(omitted_error["error"], "malformed_request");
    let (status, updated) = f.post("/api/mobile/v1/operations", update).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["committed_revision"], "2");
    record("mobile002_update_task_receipt", &updated);
    let assignee: Option<Uuid> =
        sqlx::query_scalar("SELECT assignee_user_id FROM task WHERE id=$1")
            .bind(task_id)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(assignee, None);
    let (status, task_current) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{}/tasks/{task_id}", f.person),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{task_current}");
    assert!(task_current["task"]["assignee"].is_null());
    assert_eq!(task_current["task"]["revision"], "2");
    record("mobile002_current_task", &task_current);
}

#[sqlx::test]
#[ignore]
async fn mobile002_edit_authority_scope_and_atomic_failure_matrix(pool: PgPool) {
    let f = fixture(&pool).await;
    let seed = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"authority baseline"}),
    );
    let (_, seeded) = f.post("/api/mobile/v1/operations", seed).await;
    let note_id = id(&seeded, "resource_id");

    let other_cookie = crate::common::login_cookie(&f.router, "second@fixture.test", PW).await;
    let other_install = Uuid::new_v4();
    let (status, other_bootstrap) = request(
        &f.router,
        &other_cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":other_install}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let other_context = id(&other_bootstrap, "context_id");
    let denied = json!({
        "context_id":other_context,"operation_id":Uuid::new_v4(),"kind":"edit_note",
        "device_recorded_at":"2026-09-12T20:00:00Z",
        "payload":{"person_id":f.person,"note_id":note_id,"expected_revision":"999","body":"must not disclose stale"}
    });
    let (status, forbidden) = request(
        &f.router,
        &other_cookie,
        Some(other_context),
        "POST",
        "/api/mobile/v1/operations",
        denied,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden["error"], "forbidden");

    let task_for_denial: Uuid = sqlx::query_scalar(
        "INSERT INTO task(organization_id,person_id,title,kind,assignee_user_id,created_by_user_id,origin,correlation_id) \
         VALUES($1,$2,'permission first','follow_up',$3,$3,'migration',$4) RETURNING id",
    )
    .bind(f.org)
    .bind(f.person)
    .bind(f.actor)
    .bind(Uuid::new_v4())
    .fetch_one(&f.app)
    .await
    .unwrap();
    let denied_task = json!({
        "context_id":other_context,"operation_id":Uuid::new_v4(),"kind":"update_task",
        "device_recorded_at":"2026-09-12T20:00:00Z",
        "payload":{"person_id":f.person,"task_id":task_for_denial,"expected_revision":"999","title":"must not disclose stale","kind":"call","due_at":null}
    });
    let (status, task_forbidden) = request(
        &f.router,
        &other_cookie,
        Some(other_context),
        "POST",
        "/api/mobile/v1/operations",
        denied_task,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(task_forbidden["error"], "forbidden");

    // A foreign Organization target is indistinguishable from a missing one
    // on both edit kinds and both comparison reads.
    let (foreign_org, foreign_actor) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Foreign Mobile",
        "foreign-mobile@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    let foreign_stage: Uuid = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1",
    )
    .bind(foreign_org)
    .fetch_one(&f.app)
    .await
    .unwrap();
    let foreign_person: Uuid = sqlx::query_scalar("INSERT INTO person(organization_id,first_name,stage_id) VALUES($1,'Foreign',$2) RETURNING id")
        .bind(foreign_org).bind(foreign_stage).fetch_one(&f.app).await.unwrap();
    let foreign_note: Uuid = sqlx::query_scalar("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) VALUES($1,$2,$3,'foreign note','migration',$4) RETURNING id")
        .bind(foreign_org).bind(foreign_person).bind(foreign_actor).bind(Uuid::new_v4()).fetch_one(&f.app).await.unwrap();
    let foreign_task: Uuid = sqlx::query_scalar("INSERT INTO task(organization_id,person_id,title,kind,assignee_user_id,created_by_user_id,origin,correlation_id) VALUES($1,$2,'foreign task','follow_up',$3,$3,'migration',$4) RETURNING id")
        .bind(foreign_org).bind(foreign_person).bind(foreign_actor).bind(Uuid::new_v4()).fetch_one(&f.app).await.unwrap();
    for operation in [
        f.operation("edit_note", json!({"person_id":foreign_person,"note_id":foreign_note,"expected_revision":"1","body":"no cross tenant"})),
        f.operation("update_task", json!({"person_id":foreign_person,"task_id":foreign_task,"expected_revision":"1","title":"no cross tenant","kind":"call","due_at":null})),
    ] {
        assert_eq!(f.post("/api/mobile/v1/operations", operation).await.0, StatusCode::NOT_FOUND);
    }
    for path in [
        format!("/api/mobile/v1/people/{foreign_person}/notes/{foreign_note}"),
        format!("/api/mobile/v1/people/{foreign_person}/tasks/{foreign_task}"),
    ] {
        assert_eq!(
            request(
                &f.router,
                &f.cookie,
                Some(f.context),
                "GET",
                &path,
                json!({})
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }

    // A record can never be reached through another Person path, even in the
    // same Organization; this also pins the generic visibility 404.
    let stage: Uuid = sqlx::query_scalar("SELECT stage_id FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let another_person: Uuid = sqlx::query_scalar(
        "INSERT INTO person(organization_id,first_name,stage_id) VALUES($1,'Other',$2) RETURNING id",
    ).bind(f.org).bind(stage).fetch_one(&f.app).await.unwrap();
    let wrong_person = f.operation("edit_note", json!({"person_id":another_person,"note_id":note_id,"expected_revision":"1","body":"wrong path"}));
    let (status, missing) = f.post("/api/mobile/v1/operations", wrong_person).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["error"], "not_found");
    let (status, hidden) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{another_person}/notes/{note_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(hidden["error"], "not_found");

    // An authorized exact baseline no-op receives a durable receipt at the
    // unchanged version; a stale baseline conflicts before equality checks.
    let noop = f.operation("edit_note", json!({"person_id":f.person,"note_id":note_id,"expected_revision":"1","body":"authority baseline"}));
    let (status, noop_receipt) = f.post("/api/mobile/v1/operations", noop).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noop_receipt["changed"], false);
    assert_eq!(noop_receipt["committed_revision"], "1");
    let task_create = f.operation("create_task", json!({"person_id":f.person,"title":"completed edit","kind":"follow_up","due_at":null,"assignee_user_id":null}));
    let (_, created) = f.post("/api/mobile/v1/operations", task_create).await;
    let task_id = id(&created, "resource_id");
    task::complete_task(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&f.auth()),
        task::CompleteTask {
            person_id: PersonId(f.person),
            task_id: TaskId(task_id),
        },
    )
    .await
    .unwrap();
    let stale_task = f.operation("update_task", json!({"person_id":f.person,"task_id":task_id,"expected_revision":"1","title":"completed edit","kind":"follow_up","due_at":null}));
    let (status, conflict) = f.post("/api/mobile/v1/operations", stale_task).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"], "revision_conflict");
    let complete_edit = f.operation("update_task", json!({"person_id":f.person,"task_id":task_id,"expected_revision":"2","title":"completed retitled","kind":"call","due_at":null}));
    let (status, completed_receipt) = f.post("/api/mobile/v1/operations", complete_edit).await;
    assert_eq!(status, StatusCode::OK, "{completed_receipt}");
    let completed: bool =
        sqlx::query_scalar("SELECT completed_at IS NOT NULL FROM task WHERE id=$1")
            .bind(task_id)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert!(completed);

    // An accepted edit cannot be resurrected by replay after deletion.
    let accepted = f.operation("edit_note", json!({"person_id":f.person,"note_id":note_id,"expected_revision":"1","body":"accepted then deleted"}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", accepted.clone())
            .await
            .0,
        StatusCode::OK
    );
    note::delete_note(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&f.auth()),
        note::DeleteNote {
            person_id: PersonId(f.person),
            note_id: NoteId(note_id),
        },
    )
    .await
    .unwrap();
    let (status, replay_deleted) = f.post("/api/mobile/v1/operations", accepted).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(replay_deleted["error"], "not_found");

    // Receipt insertion failure rolls back the business body/version/person
    // revision with the same transaction; no split acceptance is possible.
    let rollback_seed = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"rollback baseline"}),
    );
    let (_, rollback_created) = f.post("/api/mobile/v1/operations", rollback_seed).await;
    let rollback_note = id(&rollback_created, "resource_id");
    let before_person: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION mobile002_fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic receipt failure'; END $$; CREATE TRIGGER mobile002_receipt_failure BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile002_fail_receipt();").execute(&pool).await.unwrap();
    let rollback = f.operation("edit_note", json!({"person_id":f.person,"note_id":rollback_note,"expected_revision":"1","body":"must roll back"}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", rollback.clone())
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    sqlx::query("DROP TRIGGER mobile002_receipt_failure ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT body FROM note WHERE id=$1")
            .bind(rollback_note)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        "rollback baseline"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM note WHERE id=$1")
            .bind(rollback_note)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT mobile_revision FROM person WHERE id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        before_person
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_operation_receipt WHERE operation_id=$1"
        )
        .bind(id(&rollback, "operation_id"))
        .fetch_one(&f.app)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn mobile004_stage_receipts_catalog_and_review_hold(pool: PgPool) {
    let f = fixture(&pool).await;
    let stages: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position,id LIMIT 2",
    )
    .bind(f.org)
    .fetch_all(&f.app)
    .await
    .unwrap();
    let initial = stages[0];
    let target = stages[1];
    let accepted = f.operation(
        "change_person_stage",
        json!({"person_id":f.person,"stage_id":target,"expected_stage_revision":"1"}),
    );
    let (status, receipt) = f.post("/api/mobile/v1/operations", accepted.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["resource_type"], "person_stage");
    assert_eq!(receipt["resource_id"], f.person.to_string());
    assert_eq!(receipt["committed_revision"], "2");
    assert_eq!(receipt["changed"], true);

    note::add_note(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&f.auth()),
        note::AddNote {
            person_id: PersonId(f.person),
            body: note::NoteBody::parse("unrelated note").unwrap(),
        },
    )
    .await
    .unwrap();
    let no_op = f.operation(
        "change_person_stage",
        json!({"person_id":f.person,"stage_id":target,"expected_stage_revision":"2"}),
    );
    let (status, no_op_receipt) = f.post("/api/mobile/v1/operations", no_op).await;
    assert_eq!(status, StatusCode::OK, "{no_op_receipt}");
    assert_eq!(no_op_receipt["changed"], false);
    assert_eq!(no_op_receipt["committed_revision"], "2");

    crm_api::domain::commands::change_person_stage(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&f.auth()),
        crm_api::domain::commands::ChangePersonStage {
            person_id: PersonId(f.person),
            stage_id: crm_api::ids::StageId(initial),
        },
    )
    .await
    .unwrap();
    let aba = f.operation(
        "change_person_stage",
        json!({"person_id":f.person,"stage_id":initial,"expected_stage_revision":"1"}),
    );
    let (status, conflict) = f.post("/api/mobile/v1/operations", aba).await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(conflict["error"], "revision_conflict");
    let (status, replay) = f.post("/api/mobile/v1/operations", accepted).await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["committed_revision"], "2");

    let (status, current) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{}/stage", f.person),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(current["stage"]["id"], initial.to_string());
    assert_eq!(current["stage_revision"], "3");

    let before_catalog: i64 =
        sqlx::query_scalar("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.app)
            .await
            .unwrap();
    for position in 100..=201_i16 {
        sqlx::query("INSERT INTO stage(organization_id,name,position) VALUES($1,$2,$3)")
            .bind(f.org)
            .bind(format!("Mobile004 {position}"))
            .bind(position)
            .execute(&f.app)
            .await
            .unwrap();
    }
    let after_catalog: i64 =
        sqlx::query_scalar("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(after_catalog, before_catalog + 102);
    assert!(sqlx::query(
        "UPDATE organization SET stage_catalog_revision=stage_catalog_revision+1 WHERE id=$1"
    )
    .bind(f.org)
    .execute(&f.app)
    .await
    .is_err());
    let (status, generation) = f
        .post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[],"include_stage_catalog":true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{generation}");
    let generation_id = id(&generation, "generation_id");
    let (status, first) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/reconciliations/{generation_id}/stages"),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["items"].as_array().unwrap().len(), 100);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (status, second) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/reconciliations/{generation_id}/stages?cursor={cursor}"),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["complete"], true);
    sqlx::query("UPDATE stage SET name='Mobile004 renamed' WHERE organization_id=$1 AND id=$2")
        .bind(f.org)
        .bind(initial)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{generation_id}/stages"),
            json!(null)
        )
        .await
        .1["error"],
        "generation_changed"
    );

    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org).execute(&pool).await.unwrap();
    let held = f.operation(
        "change_person_stage",
        json!({"person_id":f.person,"stage_id":target,"expected_stage_revision":"3"}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", held).await.1["error"],
        "workspace_in_migration_review"
    );
}

#[sqlx::test]
#[ignore]
async fn current_authority_cross_org_receipts_and_workspace_hold(pool: PgPool) {
    let f = fixture(&pool).await;
    let (foreign_org, _) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Foreign",
        "foreign@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    let stage: Uuid = sqlx::query_scalar("SELECT id FROM stage WHERE organization_id=$1 LIMIT 1")
        .bind(foreign_org)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO person(organization_id,stage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(foreign_org)
    .bind(stage)
    .fetch_one(&f.app)
    .await
    .unwrap();
    for person in [foreign, Uuid::new_v4()] {
        assert_eq!(
            f.post(
                "/api/mobile/v1/operations",
                f.operation("add_note", json!({"person_id":person,"body":"Denied"}))
            )
            .await
            .1["error"],
            "not_found"
        );
        assert_eq!(f.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[person]})).await.1["error"],"not_found");
    }
    let create=f.operation("create_task",json!({"person_id":f.person,"title":"Shared","kind":"other","due_at":null,"assignee_user_id":null}));
    let (_, created) = f.post("/api/mobile/v1/operations", create).await;
    let task = id(&created, "resource_id");
    let complete = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"task_id":task,"expected_revision":"1"}}),
    );
    let (_, accepted) = f.post("/api/mobile/v1/operations", complete.clone()).await;
    // A prior receipt remains replayable after losing manage permission.
    sqlx::query("UPDATE task SET created_by_user_id=$1,assignee_user_id=$1 WHERE id=$2")
        .bind(f.other)
        .bind(task)
        .execute(&f.app)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", complete.clone())
            .await
            .1["replayed"],
        true
    );
    let new = f.operation(
        "complete_task",
        json!({"person_id":f.person,"target":{"task_id":task,"expected_revision":"1"}}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", new).await.0,
        StatusCode::FORBIDDEN
    );
    let other_cookie = crate::common::login_cookie(&f.router, "second@fixture.test", PW).await;
    assert_eq!(
        request(
            &f.router,
            &other_cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/operations/{}",
                accepted["operation_id"].as_str().unwrap()
            ),
            json!(null)
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    task::delete_task(
        &f.app,
        &Publisher::recording(),
        &CommandContext::from_auth(&AuthContext {
            actor_user_id: UserId(f.other),
            ..f.auth()
        }),
        task::DeleteTask {
            person_id: PersonId(f.person),
            task_id: TaskId(task),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", complete).await.1["error"],
        "not_found"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_operation_receipt WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            None,
            "POST",
            "/api/mobile/v1/bootstrap",
            json!({"protocol":"mobile-v1","installation_id":f.install})
        )
        .await
        .1["error"],
        "workspace_in_migration_review"
    );
}

#[sqlx::test]
#[ignore]
async fn bounded_downloads_revisions_seal_and_acknowledgment_order(pool: PgPool) {
    let f = fixture(&pool).await;
    // Representative approved fixture: 100 selected People, 1,000 notes/tasks.
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Synthetic '||n,p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,99) n WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) SELECT $1,$2,$3,'Synthetic note '||n,'web_session',gen_random_uuid() FROM generate_series(1,1000) n")
        .bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,created_by_user_id,assignee_user_id,origin,correlation_id) SELECT $1,$2,'Synthetic task '||n,'follow_up',$3,$3,'web_session',gen_random_uuid() FROM generate_series(1,1000) n")
        .bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap();
    let generation = f.gen().await;
    assert_eq!(generation["selected_count"], 100);
    assert_eq!(generation["complete"], true);
    record("reconciliation", &generation);
    record("reconciliation_bootstrap", &f.bootstrap);
    let gen = id(&generation, "generation_id");
    let (status, summary) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!(
            "/api/mobile/v1/reconciliations/{gen}/people/{}/summary",
            f.person
        ),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    record("summary_page", &summary);
    for section in ["notes", "tasks"] {
        let mut cursor = None;
        let mut count = 0;
        let mut pages = 0;
        loop {
            let url = format!(
                "/api/mobile/v1/reconciliations/{gen}/people/{}/{section}{}",
                f.person,
                cursor
                    .as_ref()
                    .map(|c| format!("?cursor={c}"))
                    .unwrap_or_default()
            );
            let (status, page) = request(
                &f.router,
                &f.cookie,
                Some(f.context),
                "GET",
                &url,
                json!(null),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{page}");
            if pages == 0 {
                record(&format!("{section}_page"), &page);
            }
            let rows = page["items"].as_array().unwrap();
            assert!(rows.len() <= 100);
            assert!(page.to_string().len() <= 524288);
            if section == "tasks" {
                assert!(rows.iter().all(|r| r["revision"] == "1"));
            }
            count += rows.len();
            pages += 1;
            cursor = page["next_cursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                assert_eq!(page["complete"], true);
                break;
            }
            assert_eq!(page["complete"], false);
            assert!(pages <= 10);
        }
        assert_eq!(count, 1000);
        assert_eq!(pages, 10);
    }
    let (status, sealed) = f
        .post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{sealed}");
    record("seal", &sealed);
    // The server fixed-clock Today result is exactly the existing normal core.
    let evaluated: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(generation["evaluated_at"].clone()).unwrap();
    let today = crm_api::domain::today::query_at(
        &mut f.app.acquire().await.unwrap(),
        &crm_api::domain::person::visibility::PersonVisibilityScope::Organization(OrganizationId(
            f.org,
        )),
        UserId(f.actor),
        evaluated,
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(today).unwrap(), sealed["today"]);
    let entry = generation["manifest"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["person_id"] == f.person.to_string())
        .unwrap();
    let old_revision = entry["revision"].as_str().unwrap().parse::<i64>().unwrap();
    // A response lost after commit is retried under its immutable ID; 100
    // queued actions each settle once. Old sealed bundles cannot cover them.
    for n in 0..100 {
        let op = f.operation(
            "add_note",
            json!({"person_id":f.person,"body":format!("Offline queued {n}")}),
        );
        let (status, receipt) = f.post("/api/mobile/v1/operations", op.clone()).await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert!(
            receipt["person_revision"]
                .as_str()
                .unwrap()
                .parse::<i64>()
                .unwrap()
                > old_revision
        );
        assert_eq!(
            f.post("/api/mobile/v1/operations", op).await.1["replayed"],
            true
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1100
    );
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({})
        )
        .await
        .1["error"],
        "generation_changed"
    );
    let (status, changed) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!(
            "/api/mobile/v1/reconciliations/{gen}/people/{}/notes",
            f.person
        ),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(changed["error"], "generation_changed");
    record("generation_changed", &changed);
    // Label fanout includes note authors even when no Person is assigned.
    let before: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    sqlx::query("UPDATE app_user SET display_name='Changed Agent' WHERE id=$1")
        .bind(f.actor)
        .execute(&pool)
        .await
        .unwrap();
    let after: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert!(after > before);
    // Every exact response shape can be fed to native contract fixture readers.
    let (status, error) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        "/api/mobile/v1/reconciliations/not-a-uuid/manifest",
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "malformed_request");
}

#[sqlx::test]
#[ignore]
async fn trusted_admission_context_rotation_cleanup_and_cursor_scope(pool: PgPool) {
    let f = fixture(&pool).await;
    // At least two manifest pages and no dependence on caller-generated IDs.
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Manifest '||n,p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,260) n WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    let first = f.gen().await;
    let first_id = id(&first, "generation_id");
    assert_eq!(first["manifest"]["items"].as_array().unwrap().len(), 250);
    assert_eq!(first["manifest"]["complete"], false);
    let second = f.gen().await;
    let second_id = id(&second, "generation_id");
    let first_cursor = first["manifest"]["next_cursor"].as_str().unwrap();
    let (status, page) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/reconciliations/{first_id}/manifest?cursor={first_cursor}"),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 11);
    assert_eq!(page["complete"], true);
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{second_id}/manifest?cursor={first_cursor}"),
            json!(null)
        )
        .await
        .1["error"],
        "malformed_request"
    );
    assert_eq!(
        f.post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    for n in 0..9 {
        let installation = Uuid::new_v4();
        let (status, boot) = request(
            &f.router,
            &f.cookie,
            None,
            "POST",
            "/api/mobile/v1/bootstrap",
            json!({"protocol":"mobile-v1","installation_id":installation}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let context = id(&boot, "context_id");
        let (status, _) = request(
            &f.router,
            &f.cookie,
            Some(context),
            "POST",
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":installation,"pinned_person_ids":[]}),
        )
        .await;
        assert_eq!(
            status,
            if n < 2 {
                StatusCode::OK
            } else {
                StatusCode::TOO_MANY_REQUESTS
            }
        );
    }
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            None,
            "POST",
            "/api/mobile/v1/bootstrap",
            json!({"protocol":"mobile-v1","installation_id":Uuid::new_v4()})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        4
    );
    sqlx::query("UPDATE mobile_reconciliation SET expires_at=now()-interval '1 second' WHERE organization_id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{first_id}/manifest"),
            json!(null)
        )
        .await
        .1["error"],
        "generation_expired"
    );
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn query_budget_rollback_nullable_attribution_and_physical_delete(pool: PgPool) {
    let f = fixture(&pool).await;
    // The whole-request budget includes session extraction before mobile routes.
    let mut session_lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE user_session IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *session_lock)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let (status, error) = request(
        &f.router,
        &f.cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":f.install}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"], "unavailable");
    assert!(started.elapsed() >= std::time::Duration::from_secs(19));
    assert!(started.elapsed() < std::time::Duration::from_secs(25));
    session_lock.rollback().await.unwrap();

    // Legacy null attribution is legal in ordinary stored data; reads still
    // need concrete boolean permission hints for members.
    sqlx::query("INSERT INTO note(organization_id,person_id,body,origin,correlation_id) VALUES($1,$2,'Unmatched historic author','migration',gen_random_uuid())").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,origin,correlation_id) VALUES($1,$2,'Unmatched historic creator','other','migration',gen_random_uuid())").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let (status,generation)=f.post("/api/mobile/v1/reconciliations",json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[f.person]})).await;
    assert_eq!(status, StatusCode::OK);
    let gen = id(&generation, "generation_id");
    for section in ["notes", "tasks"] {
        let (status, page) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/reconciliations/{gen}/people/{}/{section}",
                f.person
            ),
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["items"][0]["can_manage"], false);
    }
    let op = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"Slow receipt stays pending"}),
    );
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_slow_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(10); RETURN NEW; END $$; CREATE TRIGGER mobile_fixture_slow BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile_fixture_slow_receipt();").execute(&pool).await.unwrap();
    let started = std::time::Instant::now();
    assert_eq!(
        f.post("/api/mobile/v1/operations", op.clone()).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(8));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    sqlx::query("DROP TRIGGER mobile_fixture_slow ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    let (_, accepted) = f.post("/api/mobile/v1/operations", op.clone()).await;
    sqlx::query("DELETE FROM person WHERE id=$1")
        .bind(f.person)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", op).await.1["error"],
        "not_found"
    );
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/operations/{}",
                accepted["operation_id"].as_str().unwrap()
            ),
            json!(null)
        )
        .await
        .1["error"],
        "not_found"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_operation_receipt WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        1
    );
    let (_, changed) = f
        .post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({}),
        )
        .await;
    assert_eq!(changed["error"], "generation_changed");
    assert_eq!(changed["changes"]["removed"], 1);
    record("removed_pinned_person", &changed);
}

#[sqlx::test]
#[ignore]
async fn maximum_selection_plan_and_failed_cleanup_stay_bounded(pool: PgPool) {
    let f = fixture(&pool).await;
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id,assigned_user_id) SELECT $1,'Scale '||n,p.stage_id,$2 FROM person p CROSS JOIN generate_series(1,24999) n WHERE p.id=$3")
        .bind(f.org).bind(f.actor).bind(f.person).execute(&f.app).await.unwrap();
    sqlx::query("ANALYZE person").execute(&pool).await.unwrap();
    let pins: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM person WHERE organization_id=$1 ORDER BY id")
            .bind(f.org)
            .fetch_all(&f.app)
            .await
            .unwrap();
    let started = std::time::Instant::now();
    let (status, generation) = f
        .post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":pins}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{generation}");
    assert_eq!(generation["selected_count"], 25000);
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let plan:Value=sqlx::query_scalar("EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON) SELECT id,mobile_revision FROM person WHERE organization_id=$1 AND (assigned_user_id=$2 OR id=ANY($3::uuid[]) OR id=ANY($4::uuid[])) ORDER BY id LIMIT 25001")
        .bind(f.org).bind(f.actor).bind(&pins).bind(Vec::<Uuid>::new()).fetch_one(&f.app).await.unwrap();
    record(
        "selection_25000_plan",
        &json!({"planning_ms":plan[0]["Planning Time"],"execution_ms":plan[0]["Execution Time"],"root_node":plan[0]["Plan"]["Node Type"],"actual_rows":plan[0]["Plan"]["Actual Rows"],"shared_hit_blocks":plan[0]["Plan"]["Shared Hit Blocks"],"shared_read_blocks":plan[0]["Plan"]["Shared Read Blocks"]}),
    );
    assert_eq!(plan[0]["Plan"]["Actual Rows"].as_f64(), Some(25000.0));
    sqlx::query("INSERT INTO person(organization_id,stage_id,assigned_user_id) SELECT organization_id,stage_id,assigned_user_id FROM person WHERE id=$1").bind(f.person).execute(&f.app).await.unwrap();
    assert_eq!(
        f.post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]})
        )
        .await
        .1["error"],
        "over_limit"
    );
    sqlx::query("UPDATE mobile_reconciliation SET expires_at=now()-interval '1 second' WHERE organization_id=$1").bind(f.org).execute(&pool).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_cleanup_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic cleanup failure'; END $$; CREATE TRIGGER mobile_fixture_cleanup BEFORE DELETE ON mobile_reconciliation FOR EACH ROW EXECUTE FUNCTION mobile_fixture_cleanup_failure();").execute(&pool).await.unwrap();
    assert!(mobile::cleanup_once(&f.app).await.is_err());
    assert_eq!(
        f.post(
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]})
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_reconciliation_person")
            .fetch_one(&f.app)
            .await
            .unwrap(),
        25000
    );
    sqlx::query("DROP TRIGGER mobile_fixture_cleanup ON mobile_reconciliation")
        .execute(&pool)
        .await
        .unwrap();
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mobile_reconciliation_person")
            .fetch_one(&f.app)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn concurrent_admission_and_download_slots_are_server_enforced(pool: PgPool) {
    let f = fixture(&pool).await;
    let body = json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]});
    let (a, b, c) = tokio::join!(
        f.post("/api/mobile/v1/reconciliations", body.clone()),
        f.post("/api/mobile/v1/reconciliations", body.clone()),
        f.post("/api/mobile/v1/reconciliations", body)
    );
    let results = [a, b, c];
    let successes = results.iter().filter(|r| r.0 == StatusCode::OK).count();
    assert!((1..=2).contains(&successes));
    for result in &results {
        assert!([
            StatusCode::OK,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::TOO_MANY_REQUESTS
        ]
        .contains(&result.0));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1"
        )
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        successes as i64
    );
    let gen = id(
        &results.iter().find(|r| r.0 == StatusCode::OK).unwrap().1,
        "generation_id",
    );
    let mut held = f.app.begin().await.unwrap();
    for slot in ["0", "1"] {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('crm-mobile-download:'||$1::text||':'||$2::text,0))").bind(f.context).bind(slot).execute(&mut *held).await.unwrap();
    }
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{gen}/manifest"),
            json!(null)
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    held.rollback().await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{gen}/manifest"),
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[sqlx::test]
#[ignore]
async fn derived_trigger_permissions_never_bypass_review_business_guards(pool: PgPool) {
    let f = fixture(&pool).await;
    let before: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    let direct = sqlx::query("UPDATE person SET mobile_revision=mobile_revision+1 WHERE id=$1")
        .bind(f.person)
        .execute(&f.app)
        .await
        .unwrap_err();
    assert!(crm_api::auth::workspace::is_review_error(&direct));
    let insert=sqlx::query("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) VALUES($1,$2,$3,'Must not bypass','mobile_session',gen_random_uuid())").bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap_err();
    assert!(crm_api::auth::workspace::is_review_error(&insert));
    let after: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    assert_eq!(before, after);
    let can_call: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('crm_app','crm_mobile_component_revision()','EXECUTE')",
    )
    .fetch_one(&f.app)
    .await
    .unwrap();
    assert!(!can_call);
}

#[sqlx::test]
#[ignore]
async fn sealed_generations_reclaim_and_recover_lost_seals_without_losing_receipts(pool: PgPool) {
    let f = fixture(&pool).await;
    let operation = f.operation(
        "add_note",
        json!({"person_id":f.person,"body":"Retained across repeated sync"}),
    );
    let (status, receipt) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let receipt_rows: Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(r)) FROM mobile_operation_receipt r WHERE organization_id=$1",
    )
    .bind(f.org)
    .fetch_one(&f.app)
    .await
    .unwrap();
    let installation = Uuid::new_v4();
    let (status, boot) = request(
        &f.router,
        &f.cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":installation}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_context = id(&boot, "context_id");
    async fn create(f: &Fixture, context: Uuid, installation: Uuid) -> Value {
        let (status, value) = request(
            &f.router,
            &f.cookie,
            Some(context),
            "POST",
            "/api/mobile/v1/reconciliations",
            json!({"protocol":"mobile-v1","installation_id":installation,"pinned_person_ids":[]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        value
    }
    let pending = id(
        &create(&f, second_context, installation).await,
        "generation_id",
    );
    let mut lost_generation = None;
    let mut cached_revision = Value::Null;
    for (context, installation) in [(f.context, f.install), (second_context, installation)] {
        for cycle in 0..6 {
            let generation = create(&f, context, installation).await;
            let gen = id(&generation, "generation_id");
            let revision = generation["manifest"]["items"][0]["revision"].clone();
            if cached_revision.is_null() {
                cached_revision = revision.clone();
            }
            assert_eq!(
                revision, cached_revision,
                "complete unchanged bundles remain reusable"
            );
            let path = format!("/api/mobile/v1/reconciliations/{gen}/seal");
            if context == f.context && cycle == 5 {
                // Drop the actual successful router response without decoding its
                // seal. The next admission may reclaim this terminal generation.
                let req = Request::builder()
                    .method("POST")
                    .uri(&path)
                    .header("content-type", "application/json")
                    .header("cookie", &f.cookie)
                    .header("X-Mobile-Context", context.to_string())
                    .body(Body::from("{}"))
                    .unwrap();
                let response = f.router.clone().oneshot(req).await.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                drop(response);
                lost_generation = Some(gen);
            } else {
                let (status, sealed) = request(
                    &f.router,
                    &f.cookie,
                    Some(context),
                    "POST",
                    &path,
                    json!({}),
                )
                .await;
                assert_eq!(status, StatusCode::OK, "{sealed}");
                let (status, replay) = request(
                    &f.router,
                    &f.cookie,
                    Some(context),
                    "POST",
                    &path,
                    json!({}),
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(
                    replay, sealed,
                    "retained seal retries revalidate without changing their timestamp"
                );
            }
            assert!(sqlx::query_scalar::<_, bool>(
                "SELECT sealed_at IS NOT NULL FROM mobile_reconciliation WHERE id=$1"
            )
            .bind(gen)
            .fetch_one(&f.app)
            .await
            .unwrap());
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
                )
                .bind(f.org)
                .fetch_one(&f.app)
                .await
                .unwrap(),
                2
            );
            assert!(sqlx::query_scalar::<_, bool>("SELECT sealed_at IS NULL AND expires_at>now() FROM mobile_reconciliation WHERE id=$1").bind(pending).fetch_one(&f.app).await.unwrap());
        }
    }
    let lost = lost_generation.unwrap();
    for (method, suffix) in [("POST", "seal"), ("GET", "manifest")] {
        let (status, error) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            method,
            &format!("/api/mobile/v1/reconciliations/{lost}/{suffix}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["error"], "not_found");
    }
    // Recovery creates a fresh generation and can use the complete bundle from
    // the lost response: selection revision is unchanged and sealing succeeds.
    let recovery = create(&f, f.context, f.install).await;
    assert_eq!(
        recovery["manifest"]["items"][0]["revision"],
        cached_revision
    );
    assert_eq!(
        f.post(
            &format!(
                "/api/mobile/v1/reconciliations/{}/seal",
                id(&recovery, "generation_id")
            ),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, replay) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["resource_id"], receipt["resource_id"]);
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT jsonb_agg(to_jsonb(r)) FROM mobile_operation_receipt r WHERE organization_id=$1").bind(f.org).fetch_one(&f.app).await.unwrap(), receipt_rows);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_context WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );

    mobile::cleanup_once(&f.app).await.unwrap(); // leaves the pending generation
    let a = id(&create(&f, f.context, f.install).await, "generation_id");
    let b = id(&create(&f, f.context, f.install).await, "generation_id");
    let c = id(
        &create(&f, second_context, installation).await,
        "generation_id",
    );
    for (context, gen) in [(f.context, a), (f.context, b), (second_context, c)] {
        assert_eq!(
            request(
                &f.router,
                &f.cookie,
                Some(context),
                "POST",
                &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
                json!({})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2,
        "one sweep removes at most two of three terminal generations"
    );
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Vec<Uuid>>(
            "SELECT array_agg(id) FROM mobile_reconciliation WHERE organization_id=$1"
        )
        .bind(f.org)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        vec![pending]
    );
    assert_eq!(sqlx::query_scalar::<_, Value>("SELECT jsonb_agg(to_jsonb(r)) FROM mobile_operation_receipt r WHERE organization_id=$1").bind(f.org).fetch_one(&f.app).await.unwrap(), receipt_rows);
}

#[sqlx::test]
#[ignore]
async fn sealed_generations_remain_counted_when_cleanup_is_locked_or_fails(pool: PgPool) {
    let f = fixture(&pool).await;
    let a = id(&f.gen().await, "generation_id");
    let b = id(&f.gen().await, "generation_id");
    // A failure while persisting the terminal marker must leave a retryable
    // pending generation, rather than publishing a seal or admitting cleanup.
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_fail_seal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic seal failure'; END $$; CREATE TRIGGER mobile_fixture_seal BEFORE UPDATE OF sealed_at ON mobile_reconciliation FOR EACH ROW EXECUTE FUNCTION mobile_fixture_fail_seal();").execute(&pool).await.unwrap();
    let path = format!("/api/mobile/v1/reconciliations/{a}/seal");
    assert_eq!(
        f.post(&path, json!({})).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT sealed_at IS NULL FROM mobile_reconciliation WHERE id=$1"
    )
    .bind(a)
    .fetch_one(&f.app)
    .await
    .unwrap());
    mobile::cleanup_once(&f.app).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1"
        )
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );
    sqlx::query("DROP TRIGGER mobile_fixture_seal ON mobile_reconciliation")
        .execute(&pool)
        .await
        .unwrap();
    for gen in [a, b] {
        assert_eq!(
            f.post(
                &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
                json!({})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let body = json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[]});
    let mut held = f.app.begin().await.unwrap();
    sqlx::query("SELECT id FROM mobile_reconciliation WHERE id=ANY($1) FOR UPDATE")
        .bind(vec![a, b])
        .fetch_all(&mut *held)
        .await
        .unwrap();
    let (status, error) = f.post("/api/mobile/v1/reconciliations", body.clone()).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error["error"], "mobile_capacity");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1"
        )
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2,
        "locked terminal rows still consume admission"
    );
    held.rollback().await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION mobile_fixture_deny_terminal_cleanup() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic terminal cleanup failure'; END $$; CREATE TRIGGER mobile_fixture_cleanup BEFORE DELETE ON mobile_reconciliation FOR EACH ROW EXECUTE FUNCTION mobile_fixture_deny_terminal_cleanup();").execute(&pool).await.unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/reconciliations", body).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1"
        )
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        2
    );
    sqlx::query("DROP TRIGGER mobile_fixture_cleanup ON mobile_reconciliation")
        .execute(&pool)
        .await
        .unwrap();
    let recovered = f.gen().await;
    assert_ne!(id(&recovered, "generation_id"), a);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1"
        )
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        1
    );
}

/// Mobile 003's reported-time exception is deliberately narrow: the fact and
/// receipt are one transaction, the Person projection token stays unchanged,
/// and retries resolve by the existing protected operation marker.
#[sqlx::test]
#[ignore]
async fn mobile003_contact_attempt_receipt_time_and_replay(pool: PgPool) {
    let f = fixture(&pool).await;
    assert!(f.bootstrap["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "log_contact_attempt"));

    let before_revision: i64 =
        sqlx::query_scalar("SELECT mobile_revision FROM person WHERE organization_id=$1 AND id=$2")
            .bind(f.org)
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap();
    let occurred_at = (chrono::Utc::now() - chrono::Duration::days(2))
        .with_nanosecond(123_456_789)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
    let operation = f.operation(
        "log_contact_attempt",
        json!({"person_id":f.person,"channel":"other","outcome":"reached","occurred_at":occurred_at}),
    );
    let (left, right) = tokio::join!(
        f.post("/api/mobile/v1/operations", operation.clone()),
        f.post("/api/mobile/v1/operations", operation.clone())
    );
    assert_eq!(left.0, StatusCode::OK, "left: {}", left.1);
    assert_eq!(right.0, StatusCode::OK, "right: {}", right.1);
    let receipt = if left.1["replayed"] == false {
        left.1
    } else {
        right.1
    };
    assert_eq!(receipt["resource_type"], "contact_attempt");
    assert!(receipt["committed_revision"].is_null());
    assert_eq!(receipt["changed"], true);
    assert_eq!(receipt["person_revision"], before_revision.to_string());
    record("mobile003_contact_attempt_receipt", &receipt);

    let fact_id = id(&receipt, "resource_id");
    let row = sqlx::query_as::<_, (String, Option<Uuid>, Option<Uuid>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
        "SELECT origin, causation_id, corrects_id, occurred_at, recorded_at FROM contact_attempted WHERE organization_id=$1 AND person_id=$2 AND id=$3",
    )
    .bind(f.org)
    .bind(f.person)
    .bind(fact_id)
    .fetch_one(&f.app)
    .await
    .unwrap();
    assert_eq!(row.0, "mobile_session");
    assert_eq!(row.1, None);
    assert_eq!(row.2, None);
    assert_eq!(row.3.timestamp_subsec_nanos(), 123_456_000);
    assert!(row.4 >= row.3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT mobile_revision FROM person WHERE organization_id=$1 AND id=$2",
        )
        .bind(f.org)
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        before_revision,
        "a contact fact is not a fabricated mobile projection revision"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM contact_attempted WHERE organization_id=$1 AND person_id=$2",
        )
        .bind(f.org)
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap(),
        1
    );

    let mut changed = operation.clone();
    changed["payload"]["occurred_at"] = json!((chrono::Utc::now() - chrono::Duration::days(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true));
    let (status, mismatch) = f.post("/api/mobile/v1/operations", changed).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(mismatch["error"], "operation_payload_mismatch");

    let future = f.operation(
        "log_contact_attempt",
        json!({"person_id":f.person,"channel":"call","outcome":"no_answer","occurred_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()}),
    );
    let (status, future_error) = f.post("/api/mobile/v1/operations", future.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(future_error["error"], "contact_time_in_future");
    record("mobile003_contact_time_in_future", &future_error);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mobile_operation_receipt WHERE organization_id=$1 AND operation_id=$2",
        )
        .bind(f.org)
        .bind(id(&future, "operation_id"))
        .fetch_one(&f.app)
        .await
        .unwrap(),
        0
    );

    // The failed future operation remains a local retry candidate.  A revised
    // operation ID/time succeeds and does not mutate the rejected marker.
    let revised = f.operation(
        "log_contact_attempt",
        json!({"person_id":f.person,"channel":"call","outcome":"no_answer","occurred_at":(chrono::Utc::now()-chrono::Duration::minutes(1)).to_rfc3339()}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", revised).await.0,
        StatusCode::OK
    );
}

/// An older offline interaction cannot satisfy a newer Inquiry just because
/// its upload happened later. Today must change through sealing, not a fake
/// Person revision bump, and an even older subsequent log cannot move maxima.
#[sqlx::test]
#[ignore]
async fn mobile003_occurrence_chronology_and_sealed_today(pool: PgPool) {
    let f = fixture(&pool).await;
    let now = chrono::Utc::now();
    let inquiry_at = now - chrono::Duration::days(1);
    sqlx::query("INSERT INTO inquiry (organization_id,person_id,raw_payload_id,source,received_at) VALUES ($1,$2,$3,'mobile003 synthetic',$4)")
        .bind(f.org).bind(f.person).bind(Uuid::new_v4()).bind(inquiry_at)
        .execute(&pool).await.unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT mobile_revision FROM person WHERE id=$1")
        .bind(f.person)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let mut generations = Vec::new();
    for (offset_hours, expected_in_today) in [(-48, true), (-1, false), (-72, false)] {
        let operation = f.operation(
            "log_contact_attempt",
            json!({
                "person_id":f.person, "channel":"other", "outcome":"no_answer",
                "occurred_at":(now + chrono::Duration::hours(offset_hours)).to_rfc3339()
            }),
        );
        let (status, receipt) = f.post("/api/mobile/v1/operations", operation).await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert_eq!(receipt["person_revision"], revision.to_string());
        let generation = f.gen().await;
        let generation_id = id(&generation, "generation_id");
        generations.push(generation_id);
        let (status, sealed) = f
            .post(
                &format!("/api/mobile/v1/reconciliations/{generation_id}/seal"),
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{sealed}");
        let present = sealed["today"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["person"]["id"] == f.person.to_string());
        assert_eq!(
            present, expected_in_today,
            "reported offset {offset_hours}: {sealed}"
        );
        let evaluated = serde_json::from_value(generation["evaluated_at"].clone()).unwrap();
        let ordinary = crm_api::domain::today::query_at(
            &mut f.app.acquire().await.unwrap(),
            &crm_api::domain::person::visibility::PersonVisibilityScope::Organization(
                OrganizationId(f.org),
            ),
            UserId(f.actor),
            evaluated,
        )
        .await
        .unwrap();
        assert_eq!(serde_json::to_value(ordinary).unwrap(), sealed["today"]);
    }
    assert!(generations.windows(2).all(|pair| pair[0] != pair[1]));
    let maximum: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT last_contact_at FROM person WHERE id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(
        maximum.timestamp_micros(),
        (now - chrono::Duration::hours(1)).timestamp_micros()
    );
}

/// Exercise the new fact-backed receipt path itself: transaction rollback,
/// current authority, exact tenant/Person binding and retained consumed IDs.
#[sqlx::test]
#[ignore]
async fn mobile003_atomic_contact_and_current_receipt_authority(pool: PgPool) {
    let f = fixture(&pool).await;
    let operation = f.operation(
        "log_contact_attempt",
        json!({
            "person_id":f.person,"channel":"text","outcome":"sent",
            "occurred_at":(chrono::Utc::now()-chrono::Duration::days(10)).to_rfc3339()
        }),
    );
    sqlx::raw_sql("CREATE FUNCTION mobile003_fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic contact receipt failure'; END $$; CREATE TRIGGER mobile003_receipt_failure BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile003_fail_receipt();")
        .execute(&pool).await.unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", operation.clone())
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let counts: (i64,i64,Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM contact_attempted WHERE person_id=$1), (SELECT count(*) FROM mobile_operation_receipt WHERE operation_id=$2), last_contact_at FROM person WHERE id=$1"
    ).bind(f.person).bind(id(&operation,"operation_id")).fetch_one(&f.app).await.unwrap();
    assert_eq!(counts, (0, 0, None));
    sqlx::query("DROP TRIGGER mobile003_receipt_failure ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    let (status, receipt) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let receipt_url = format!(
        "/api/mobile/v1/operations/{}",
        id(&operation, "operation_id")
    );
    let (status, lookup) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &receipt_url,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lookup["resource_id"], receipt["resource_id"]);
    let other_cookie = crate::common::login_cookie(&f.router, "second@fixture.test", PW).await;
    assert_eq!(
        request(
            &f.router,
            &other_cookie,
            Some(f.context),
            "GET",
            &receipt_url,
            json!(null)
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (foreign_org, _) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Contact Foreign",
        "contact-foreign@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    let foreign: Uuid = sqlx::query_scalar("INSERT INTO person (organization_id,stage_id) SELECT $1,id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1 RETURNING person.id")
        .bind(foreign_org).fetch_one(&pool).await.unwrap();
    let mut cross_org = operation.clone();
    cross_org["operation_id"] = json!(Uuid::new_v4());
    cross_org["payload"]["person_id"] = json!(foreign);
    let (status, denied) = f.post("/api/mobile/v1/operations", cross_org).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(denied["error"], "not_found");
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", operation.clone())
            .await
            .1["error"],
        "workspace_in_migration_review"
    );
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &receipt_url,
            json!(null)
        )
        .await
        .1["error"],
        "workspace_in_migration_review"
    );
    sqlx::query("UPDATE organization SET workspace_mode='operational',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    assert!(!f
        .post("/api/mobile/v1/operations", operation.clone())
        .await
        .0
        .is_success());
    assert!(!request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &receipt_url,
        json!(null)
    )
    .await
    .0
    .is_success());
    sqlx::query("UPDATE organization_membership SET status='active' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    // A fresh online authorization permits old protected work; membership
    // removal may revoke the original session, so establish a new session.
    let cookie = crate::common::login_cookie(&f.router, "mobile@fixture.test", PW).await;
    sqlx::query("DELETE FROM person WHERE id=$1")
        .bind(f.person)
        .execute(&pool)
        .await
        .unwrap();
    let (status, missing) = request(
        &f.router,
        &cookie,
        Some(f.context),
        "POST",
        "/api/mobile/v1/operations",
        operation.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    assert_eq!(missing["error"], "not_found");
    assert_eq!(
        request(
            &f.router,
            &cookie,
            Some(f.context),
            "GET",
            &receipt_url,
            json!(null)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let retained: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM mobile_operation_receipt WHERE operation_id=$1), (SELECT count(*) FROM contact_attempted WHERE id=$2)")
        .bind(id(&operation,"operation_id")).bind(id(&receipt,"resource_id")).fetch_one(&f.app).await.unwrap();
    assert_eq!(
        retained,
        (1, 1),
        "opaque absence must neither consume again nor erase the marker"
    );
}

/// New stage receipts must remain atomic and publish only committed changes.
#[sqlx::test]
#[ignore]
async fn mobile004_stage_atomic_failure_duplicate_publication_and_scope(pool: PgPool) {
    let mut f = fixture(&pool).await;
    let publisher = Publisher::recording();
    f.router = crate::common::build_router_with_publisher(&pool, publisher.clone()).await;
    let stages: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 2",
    )
    .bind(f.org)
    .fetch_all(&f.app)
    .await
    .unwrap();
    let before: (Uuid, i64, i64) =
        sqlx::query_as("SELECT stage_id,stage_revision,mobile_revision FROM person WHERE id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap();
    let operation = f.operation(
        "change_person_stage",
        json!({"person_id":f.person,"stage_id":stages[1],"expected_stage_revision":"1"}),
    );
    sqlx::raw_sql("CREATE FUNCTION mobile004_fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic stage receipt failure'; END $$; CREATE TRIGGER mobile004_receipt_failure BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile004_fail_receipt();")
        .execute(&pool).await.unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", operation.clone())
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let after: (Uuid, i64, i64) =
        sqlx::query_as("SELECT stage_id,stage_revision,mobile_revision FROM person WHERE id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(before, after);
    let counts: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM stage_changed WHERE person_id=$1),(SELECT count(*) FROM mobile_operation_receipt WHERE person_id=$1)")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    assert_eq!(counts, (0, 0));
    let Publisher::Recording(events, _) = &publisher else {
        unreachable!()
    };
    assert!(events.lock().await.is_empty());
    sqlx::query("DROP TRIGGER mobile004_receipt_failure ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    let server_before = chrono::Utc::now();
    let (left, right) = tokio::join!(
        f.post("/api/mobile/v1/operations", operation.clone()),
        f.post("/api/mobile/v1/operations", operation.clone())
    );
    assert_eq!(left.0, StatusCode::OK, "{}", left.1);
    assert_eq!(right.0, StatusCode::OK, "{}", right.1);
    assert_ne!(left.1["replayed"], right.1["replayed"]);
    assert_eq!(left.1["committed_revision"], "2");
    assert_eq!(events.lock().await.len(), 1);
    let fact: (String, String, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT origin,reason,occurred_at FROM stage_changed WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(fact.0, "mobile_session");
    assert_eq!(fact.1, "manual");
    assert!(fact.2 >= server_before);
    let noop = f.operation(
        "change_person_stage",
        json!({"person_id":f.person,"stage_id":stages[1],"expected_stage_revision":"2"}),
    );
    assert_eq!(
        f.post("/api/mobile/v1/operations", noop).await.1["changed"],
        false
    );
    assert_eq!(events.lock().await.len(), 1);
    let mut mismatch = operation.clone();
    mismatch["payload"]["stage_id"] = json!(stages[0]);
    assert_eq!(
        f.post("/api/mobile/v1/operations", mismatch).await.1["error"],
        "operation_payload_mismatch"
    );
    let (foreign_org, _) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Stage Foreign",
        "stage-foreign@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    let foreign_stage: Uuid =
        sqlx::query_scalar("SELECT id FROM stage WHERE organization_id=$1 LIMIT 1")
            .bind(foreign_org)
            .fetch_one(&pool)
            .await
            .unwrap();
    let foreign_person: Uuid = sqlx::query_scalar(
        "INSERT INTO person(organization_id,stage_id) VALUES($1,$2) RETURNING id",
    )
    .bind(foreign_org)
    .bind(foreign_stage)
    .fetch_one(&pool)
    .await
    .unwrap();
    for target_person in [foreign_person, Uuid::new_v4()] {
        let invalid = f.operation(
            "change_person_stage",
            json!({"person_id":target_person,"stage_id":stages[0],"expected_stage_revision":"2"}),
        );
        assert_eq!(
            f.post("/api/mobile/v1/operations", invalid).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request(
                &f.router,
                &f.cookie,
                Some(f.context),
                "GET",
                &format!("/api/mobile/v1/people/{target_person}/stage"),
                json!(null)
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
    for stage in [foreign_stage, Uuid::new_v4()] {
        let invalid = f.operation(
            "change_person_stage",
            json!({"person_id":f.person,"stage_id":stage,"expected_stage_revision":"2"}),
        );
        assert_eq!(
            f.post("/api/mobile/v1/operations", invalid).await.1["error"],
            "invalid_stage"
        );
    }
    let lookup = format!(
        "/api/mobile/v1/operations/{}",
        id(&operation, "operation_id")
    );
    sqlx::query("DELETE FROM person WHERE id=$1")
        .bind(f.person)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.post("/api/mobile/v1/operations", operation.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &lookup,
            json!(null)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let retained:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM stage_changed WHERE person_id=$1),(SELECT count(*) FROM mobile_operation_receipt WHERE person_id=$1)").bind(f.person).fetch_one(&f.app).await.unwrap();
    assert_eq!(retained, (1, 2));
    assert_eq!(events.lock().await.len(), 1);
}

#[sqlx::test]
#[ignore]
async fn mobile004_catalog_rollback_seal_and_current_authority(pool: PgPool) {
    let f = fixture(&pool).await;
    let create = json!({"protocol":"mobile-v1","installation_id":f.install,"pinned_person_ids":[],"include_stage_catalog":true});
    let generation = f
        .post("/api/mobile/v1/reconciliations", create.clone())
        .await
        .1;
    let generation_id = id(&generation, "generation_id");
    let revision: i64 =
        sqlx::query_scalar("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO stage(organization_id,name,position) VALUES($1,'rolled back catalog',200)",
    )
    .bind(f.org)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let still: i64 =
        sqlx::query_scalar("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revision, still);
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{generation_id}/seal"),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let legacy = f.gen().await;
    assert!(legacy.get("stage_catalog").is_none());
    let legacy_id = id(&legacy, "generation_id");
    let generation = f.post("/api/mobile/v1/reconciliations", create).await.1;
    let generation_id = id(&generation, "generation_id");
    let spare:Uuid=sqlx::query_scalar("INSERT INTO stage(organization_id,name,position) VALUES($1,'catalog deletion',201) RETURNING id").bind(f.org).fetch_one(&pool).await.unwrap();
    sqlx::query("DELETE FROM stage WHERE id=$1")
        .bind(spare)
        .execute(&pool)
        .await
        .unwrap();
    let changed: i64 =
        sqlx::query_scalar("SELECT stage_catalog_revision FROM organization WHERE id=$1")
            .bind(f.org)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(changed, revision + 2);
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{generation_id}/seal"),
            json!({})
        )
        .await
        .1["error"],
        "generation_changed"
    );
    // Catalog-only changes do not impose the new opt-in contract on old clients.
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{legacy_id}/seal"),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let stage_url = format!("/api/mobile/v1/people/{}/stage", f.person);
    let other_cookie = crate::common::login_cookie(&f.router, "second@fixture.test", PW).await;
    assert_eq!(
        request(
            &f.router,
            &other_cookie,
            Some(f.context),
            "GET",
            &stage_url,
            json!(null)
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &stage_url,
            json!(null)
        )
        .await
        .1["error"],
        "workspace_in_migration_review"
    );
    assert_eq!(
        request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!("/api/mobile/v1/reconciliations/{generation_id}/stages"),
            json!(null)
        )
        .await
        .1["error"],
        "workspace_in_migration_review"
    );
}

async fn mobile005_profile_snapshot(f: &Fixture) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('person',(SELECT to_jsonb(p) FROM person p WHERE p.id=$1),'contacts',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM contact_method c WHERE c.person_id=$1),'[]'::jsonb),'receipts',(SELECT count(*) FROM mobile_operation_receipt WHERE person_id=$1))")
        .bind(f.person).fetch_one(&f.app).await.unwrap()
}

#[sqlx::test]
#[ignore]
async fn mobile005_native_additions_keep_request_order_after_sync(pool: PgPool) {
    let f = fixture(&pool).await;
    // A future native timestamp also proves a clock adjustment cannot place new
    // methods before an existing null-import-order method.
    let existing: Uuid = sqlx::query_scalar("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,created_at) VALUES($1,$2,'email','existing@example.test','existing@example.test',now()+interval '1 day') RETURNING id")
        .bind(f.org).bind(f.person).fetch_one(&f.app).await.unwrap();
    let initial = mobile005_profile_snapshot(&f).await;
    let operation = f.operation("update_person_details", json!({
        "person_id": f.person, "expected_details_revision": initial["person"]["details_revision"].as_i64().unwrap().to_string(),
        "contact_operations":[
            {"op":"add","kind":"email","value":"first-add@example.test"},
            {"op":"edit","id":existing,"value":"retained-existing@example.test"},
            {"op":"add","kind":"email","value":"second-add@example.test"}
        ]
    }));
    let (status, receipt) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["added_contact_ids"][0]["ordinal"], 0);
    assert_eq!(receipt["added_contact_ids"][1]["ordinal"], 2);
    let expected = vec![
        existing,
        id(&receipt["added_contact_ids"][0], "id"),
        id(&receipt["added_contact_ids"][1], "id"),
    ];
    let (status, current) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("/api/mobile/v1/people/{}/details", f.person),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = current["items"].as_array().unwrap();
    assert_eq!(
        items.iter().map(|v| id(v, "id")).collect::<Vec<_>>(),
        expected
    );
    let times: Vec<chrono::DateTime<chrono::Utc>> = items
        .iter()
        .map(|v| serde_json::from_value(v["created_at"].clone()).unwrap())
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(items.iter().all(|v| v["import_order"].is_null()));
    let generation = f.gen().await;
    let gen = id(&generation, "generation_id");
    let (status, summary) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!(
            "/api/mobile/v1/reconciliations/{gen}/people/{}/summary",
            f.person
        ),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut display = summary["items"].as_array().unwrap().clone();
    display.sort_by_key(|v| {
        (
            serde_json::from_value::<chrono::DateTime<chrono::Utc>>(v["created_at"].clone())
                .unwrap(),
            id(v, "id"),
        )
    });
    assert_eq!(
        display.iter().map(|v| id(v, "id")).collect::<Vec<_>>(),
        expected
    );
    for section in ["notes", "tasks"] {
        assert_eq!(
            request(
                &f.router,
                &f.cookie,
                Some(f.context),
                "GET",
                &format!(
                    "/api/mobile/v1/reconciliations/{gen}/people/{}/{section}",
                    f.person
                ),
                json!(null)
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let after = mobile005_profile_snapshot(&f).await;
    let (status, replay) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["added_contact_ids"], receipt["added_contact_ids"]);
    assert_eq!(
        mobile005_profile_snapshot(&f).await,
        after,
        "exact replay cannot reallocate insertion order"
    );
}

#[sqlx::test]
#[ignore]
async fn mobile005_details_revision_rejects_direct_increments(pool: PgPool) {
    let f = fixture(&pool).await;
    let before = mobile005_profile_snapshot(&f).await;
    for expression in ["details_revision+1", "details_revision+2", "0"] {
        let error = sqlx::query(&format!(
            "UPDATE person SET details_revision={expression} WHERE id=$1"
        ))
        .bind(f.person)
        .execute(&f.app)
        .await
        .expect_err("unrelated direct writes cannot invent a profile revision");
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("22003")
        );
        assert_eq!(mobile005_profile_snapshot(&f).await, before);
    }
    let previous = before["person"]["details_revision"].as_i64().unwrap();
    let derived: i64 = sqlx::query_scalar("UPDATE person SET first_name='Derived name',details_revision=999999 WHERE id=$1 RETURNING details_revision")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    assert_eq!(
        derived,
        previous + 1,
        "a name change derives from OLD, never the caller value"
    );
    let contact: Uuid = sqlx::query_scalar("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) VALUES($1,$2,'email','derived@example.test','derived@example.test') RETURNING id")
        .bind(f.org).bind(f.person).fetch_one(&f.app).await.unwrap();
    assert_eq!(
        mobile005_profile_snapshot(&f).await["person"]["details_revision"],
        derived + 1
    );
    sqlx::query("UPDATE contact_method SET value='corrected@example.test',normalized_value='corrected@example.test' WHERE id=$1")
        .bind(contact).execute(&f.app).await.unwrap();
    assert_eq!(
        mobile005_profile_snapshot(&f).await["person"]["details_revision"],
        derived + 2
    );
    sqlx::query("DELETE FROM contact_method WHERE id=$1")
        .bind(contact)
        .execute(&f.app)
        .await
        .unwrap();
    assert_eq!(
        mobile005_profile_snapshot(&f).await["person"]["details_revision"],
        derived + 3
    );
}

#[sqlx::test]
#[ignore]
async fn mobile005_current_final_page_waits_for_parent_writer(pool: PgPool) {
    use std::time::{Duration, Instant};
    let f = fixture(&pool).await;
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,import_order) SELECT $1,$2,'email','page'||n||'@example.test','page'||n||'@example.test',n FROM generate_series(1,101)n")
        .bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let base = format!("/api/mobile/v1/people/{}/details", f.person);
    let (status, first) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &base,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["complete"], false);
    let url = format!("{base}?cursor={}", first["next_cursor"].as_str().unwrap());
    let mut writer = f.app.begin().await.unwrap();
    let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM person WHERE id=$1 FOR UPDATE")
        .bind(f.person)
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    let router = f.router.clone();
    let cookie = f.cookie.clone();
    let context = f.context;
    let reader = tokio::spawn(async move {
        request(&router, &cookie, Some(context), "GET", &url, json!(null)).await
    });
    let started = Instant::now();
    loop {
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid)))")
            .bind(writer_pid).fetch_one(&pool).await.unwrap();
        if blocked {
            break;
        }
        assert!(!reader.is_finished(), "final page must wait for the in-flight profile writer, not complete from an old snapshot");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "reader never acquired the parent lock"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    sqlx::query("UPDATE person SET first_name='Committed during traversal' WHERE id=$1")
        .bind(f.person)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.commit().await.unwrap();
    let (status, body) = reader.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], "revision_conflict");
    let (status, restarted) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &base,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restarted["first_name"], "Committed during traversal");
    assert_ne!(restarted["details_revision"], first["details_revision"]);
}

#[sqlx::test]
#[ignore]
async fn mobile005_revision_overflow_rolls_back_contacts_but_allows_erasure(pool: PgPool) {
    let f = fixture(&pool).await;
    let contact: Uuid = sqlx::query_scalar("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) VALUES($1,$2,'email','overflow@example.test','overflow@example.test') RETURNING id")
        .bind(f.org).bind(f.person).fetch_one(&f.app).await.unwrap();
    let mut setup = pool.begin().await.unwrap();
    sqlx::query("ALTER TABLE person DISABLE TRIGGER mobile_details_revision")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("UPDATE person SET details_revision=9223372036854775807 WHERE id=$1")
        .bind(f.person)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE person ENABLE TRIGGER mobile_details_revision")
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();
    let before = mobile005_profile_snapshot(&f).await;
    for patch in [
        json!({"first_name":"Would overflow"}),
        json!({"contact_operations":[{"op":"add","kind":"phone","value":"4155550111"}]}),
        json!({"contact_operations":[{"op":"edit","id":contact,"value":"changed@example.test"}]}),
        json!({"contact_operations":[{"op":"remove","id":contact}]}),
    ] {
        let mut payload = json!({"person_id":f.person,"expected_details_revision":i64::MAX.to_string(),"contact_operations":[]});
        payload
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let (status, error) = f
            .post(
                "/api/mobile/v1/operations",
                f.operation("update_person_details", payload),
            )
            .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{error}");
        assert_eq!(
            mobile005_profile_snapshot(&f).await,
            before,
            "revision overflow must roll back contacts and receipt together"
        );
    }
    // Erasure must not require incrementing the already-absent parent token.
    sqlx::query("DELETE FROM person WHERE id=$1 AND organization_id=$2")
        .bind(f.person)
        .bind(f.org)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM contact_method WHERE person_id=$1")
            .bind(f.person)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore]
async fn mobile005_profile_validation_swaps_and_primary_order_are_atomic(pool: PgPool) {
    let f = fixture(&pool).await;
    let contacts: Vec<Uuid> = sqlx::query_scalar("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,import_order,created_at) SELECT $1,$2,'email',v,v,n,now()-interval '2 days' FROM (VALUES('a@example.test',1),('b@example.test',2)) x(v,n) RETURNING id")
        .bind(f.org).bind(f.person).fetch_all(&f.app).await.unwrap();
    let a = contacts[0];
    let b = contacts[1];
    let initial = mobile005_profile_snapshot(&f).await;
    let revision = initial["person"]["details_revision"].as_i64().unwrap();
    let invalid = vec![
        json!({"first_name":"x".repeat(201)}),
        json!({"first_name":"bad\u{0}name"}),
        json!({"contact_operations":[{"op":"add","kind":"email","value":"x".repeat(1025)}]}),
        json!({"contact_operations":[{"op":"add","kind":"email","value":"bad\n@example.test"}]}),
        json!({"contact_operations":[{"op":"add","kind":"email","value":"A@EXAMPLE.TEST"}]}),
        json!({"contact_operations":[{"op":"edit","id":Uuid::new_v4(),"value":"x@example.test"}]}),
        json!({"contact_operations":[{"op":"edit","id":a,"value":"x@example.test"},{"op":"remove","id":a}]}),
        json!({"contact_operations":[{"op":"add","kind":"phone","value":"not a phone"}]}),
        json!({"first_name":null,"last_name":null,"contact_operations":[{"op":"remove","id":a},{"op":"remove","id":b}]}),
        json!({"first_name":"  ","last_name":"","contact_operations":[{"op":"remove","id":a},{"op":"remove","id":b}]}),
        json!({"contact_operations":vec![json!({"op":"remove","id":a});51]}),
    ];
    for (case, patch) in invalid.into_iter().enumerate() {
        let mut payload = json!({"person_id":f.person,"expected_details_revision":revision.to_string(),"first_name":"Must roll back","contact_operations":[]});
        payload
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let (status, error) = f
            .post(
                "/api/mobile/v1/operations",
                f.operation("update_person_details", payload),
            )
            .await;
        assert!(
            matches!(
                status,
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "case {case}: {status} {error}"
        );
        assert_eq!(
            mobile005_profile_snapshot(&f).await,
            initial,
            "case {case} changed persistent state"
        );
    }
    // Both stable IDs exchange normalized values. A phone add in slot2 must
    // retain that original request-array index in its receipt mapping.
    let swap = f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":revision.to_string(),"contact_operations":[
        {"op":"edit","id":a,"value":"b@example.test"},{"op":"edit","id":b,"value":"a@example.test"},
        {"op":"add","kind":"phone","value":"(555) 555-0101"}]}));
    let (status, receipt) = f.post("/api/mobile/v1/operations", swap).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert_eq!(receipt["added_contact_ids"][0]["ordinal"], 2);
    let after = mobile005_profile_snapshot(&f).await;
    for old in initial["contacts"].as_array().unwrap() {
        let current = after["contacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == old["id"])
            .unwrap();
        for field in ["id", "kind", "created_at", "import_order"] {
            assert_eq!(current[field], old[field]);
        }
    }
    // Add-before-remove is valid; the new native contact does not become the
    // primary ahead of the remaining imported contact.
    let replacement = f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":receipt["committed_revision"],"contact_operations":[
        {"op":"add","kind":"email","value":"b@example.test"},{"op":"remove","id":a}]}));
    let (status, replaced) = f.post("/api/mobile/v1/operations", replacement).await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    let primary:Uuid=sqlx::query_scalar("SELECT id FROM contact_method WHERE person_id=$1 AND kind='email' ORDER BY import_order NULLS LAST,created_at,id LIMIT 1")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    assert_eq!(primary, b);
    let residue:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM contact_method WHERE person_id=$1 AND normalized_value LIKE 'crm-details-temp:%')")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    assert!(!residue);
}

#[sqlx::test]
#[ignore]
async fn mobile005_details_receipts_publish_only_atomic_commits_and_hold_typed_noops(pool: PgPool) {
    use crm_api::domain::commands::{update_person_details_in_transaction, UpdatePersonDetails};
    let mut f = fixture(&pool).await;
    let publisher = Publisher::recording();
    f.router = crate::common::build_router_with_publisher(&pool, publisher.clone()).await;
    let operation=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":"1","first_name":"Accepted","contact_operations":[]}));
    let initial = mobile005_profile_snapshot(&f).await;
    sqlx::raw_sql("CREATE FUNCTION mobile005_receipt_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic profile receipt failure'; END $$; CREATE TRIGGER mobile005_receipt_failure BEFORE INSERT ON mobile_operation_receipt FOR EACH ROW EXECUTE FUNCTION mobile005_receipt_failure();")
        .execute(&pool).await.unwrap();
    let (status, error) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{error}");
    assert_eq!(mobile005_profile_snapshot(&f).await, initial);
    let Publisher::Recording(events, _) = &publisher else {
        unreachable!()
    };
    assert!(events.lock().await.is_empty());
    sqlx::query("DROP TRIGGER mobile005_receipt_failure ON mobile_operation_receipt")
        .execute(&pool)
        .await
        .unwrap();
    let (left, right) = tokio::join!(
        f.post("/api/mobile/v1/operations", operation.clone()),
        f.post("/api/mobile/v1/operations", operation.clone())
    );
    assert_eq!(left.0, StatusCode::OK, "{}", left.1);
    assert_eq!(right.0, StatusCode::OK, "{}", right.1);
    assert_ne!(left.1["replayed"], right.1["replayed"]);
    let publications = events.lock().await;
    assert_eq!(publications.len(), 1);
    assert_eq!(
        publications[0].1["data"],
        json!({"person_id":f.person,"change":"details_changed"})
    );
    drop(publications);
    let noop=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":"2","first_name":"Accepted","contact_operations":[]}));
    let (status, unchanged) = f.post("/api/mobile/v1/operations", noop.clone()).await;
    assert_eq!(status, StatusCode::OK, "{unchanged}");
    assert_eq!(unchanged["changed"], false);
    assert_eq!(unchanged["committed_revision"], "2");
    assert_eq!(unchanged["added_contact_ids"], json!([]));
    assert_eq!(left.1["added_contact_ids"], json!([]));
    assert_eq!(right.1["added_contact_ids"], json!([]));

    assert_eq!(events.lock().await.len(), 1);
    sqlx::query("UPDATE person SET first_name='Other' WHERE id=$1")
        .bind(f.person)
        .execute(&f.app)
        .await
        .unwrap();
    sqlx::query("UPDATE person SET first_name='Accepted' WHERE id=$1")
        .bind(f.person)
        .execute(&f.app)
        .await
        .unwrap();
    let mut stale = noop;
    stale["operation_id"] = json!(Uuid::new_v4());
    let (status, conflict) = f.post("/api/mobile/v1/operations", stale).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"], "revision_conflict");
    let (status, replay) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["committed_revision"], "2");
    let mut mismatch = operation;
    mismatch["payload"]["first_name"] = json!("Different");
    assert_eq!(
        f.post("/api/mobile/v1/operations", mismatch).await.1["error"],
        "operation_payload_mismatch"
    );
    assert_eq!(events.lock().await.len(), 1);
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(f.org)
    .bind(f.actor)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.org).execute(&pool).await.unwrap();
    let held=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":"4","first_name":"Accepted","contact_operations":[]}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", held).await.1["error"],
        "workspace_in_migration_review"
    );
    let mut tx = f.app.begin().await.unwrap();
    let error = update_person_details_in_transaction(
        &mut tx,
        &CommandContext::from_auth(&f.auth()),
        UpdatePersonDetails {
            person_id: PersonId(f.person),
            expected_details_revision: 4,
            first_name: Some(Some("Accepted".into())),
            last_name: None,
            contact_operations: Vec::new(),
        },
    )
    .await
    .err()
    .expect("direct typed no-op must retain the review hold");
    assert!(
        matches!(error,crm_api::domain::commands::CommandError::Database(ref e) if crm_api::auth::workspace::is_review_error(e))
    );
    tx.rollback().await.unwrap();
}

#[sqlx::test]
#[ignore]
async fn mobile005_profile_pages_bound_bytes_pin_revision_and_preserve_lease(pool: PgPool) {
    let f = fixture(&pool).await;
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value,import_order,created_at) SELECT $1,$2,'email',repeat('x',8000)||n,'contact'||n||'@example.test',CASE WHEN n<=110 THEN n ELSE NULL END,now()+make_interval(secs=>n) FROM generate_series(1,151)n")
        .bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let expected:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM contact_method WHERE person_id=$1 ORDER BY import_order NULLS LAST,created_at,id")
        .bind(f.person).fetch_all(&f.app).await.unwrap();
    let lease: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM mobile_context c WHERE id=$1")
        .bind(f.context)
        .fetch_one(&f.app)
        .await
        .unwrap();
    let base = format!("/api/mobile/v1/people/{}/details", f.person);
    let mut cursor: Option<String> = None;
    let mut seen = Vec::new();
    let mut pages = 0;
    let mut first_cursor = None;
    let mut pinned = String::new();
    loop {
        let url = format!(
            "{base}{}",
            cursor
                .as_ref()
                .map(|v| format!("?cursor={v}"))
                .unwrap_or_default()
        );
        let (status, page) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &url,
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.to_string().len() <= 524288);
        assert_eq!(page["person_id"], f.person.to_string());
        assert_eq!(page["context_id"], f.context.to_string());
        let revision = page["details_revision"].as_str().unwrap().to_owned();
        if pages == 0 {
            pinned = revision.clone();
        }
        assert_eq!(revision, pinned);
        let items = page["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.len() <= 100);
        seen.extend(items.iter().map(|item| id(item, "id")));
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        pages += 1;
        if pages == 1 {
            first_cursor = cursor.clone();
        }
        if cursor.is_none() {
            assert_eq!(page["complete"], true);
            break;
        }
        assert_eq!(page["complete"], false);
        assert!(pages < 5);
    }
    assert!(
        pages >= 3,
        "the byte bound must shorten pages below 100 rows"
    );
    assert_eq!(seen, expected);
    let after_lease: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM mobile_context c WHERE id=$1")
            .bind(f.context)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(lease, after_lease);
    let generations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mobile_reconciliation WHERE context_id=$1")
            .bind(f.context)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(generations, 0, "current reads never promote a download");
    let generation = f.gen().await;
    let gen = id(&generation, "generation_id");
    let summary_base = format!(
        "/api/mobile/v1/reconciliations/{gen}/people/{}/summary",
        f.person
    );
    let mut cursor: Option<String> = None;
    let mut generation_contacts = Vec::new();
    loop {
        let url = format!(
            "{summary_base}{}",
            cursor
                .as_ref()
                .map(|v| format!("?cursor={v}"))
                .unwrap_or_default()
        );
        let (status, page) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &url,
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert!(page.to_string().len() <= 524288);
        assert_eq!(page["summary"]["details_revision"], pinned);
        for item in page["items"].as_array().unwrap() {
            assert!(item.get("import_order").is_some());
            assert!(item["created_at"].is_string());
            generation_contacts.push(id(item, "id"));
        }
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            assert_eq!(page["complete"], true);
            break;
        }
    }
    let mut ordered = expected.clone();
    ordered.sort();
    assert_eq!(
        generation_contacts, ordered,
        "legacy summary paging remains UUID ordered"
    );
    for section in ["notes", "tasks"] {
        let (status, _) = request(
            &f.router,
            &f.cookie,
            Some(f.context),
            "GET",
            &format!(
                "/api/mobile/v1/reconciliations/{gen}/people/{}/{section}",
                f.person
            ),
            json!(null),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    assert_eq!(
        f.post(
            &format!("/api/mobile/v1/reconciliations/{gen}/seal"),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    // Profile edit must preserve every untouched imported value over the edit
    // limit; the old page cursor must then conflict before exposing new rows.
    let operation=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":pinned,"last_name":"Retained","contact_operations":[]}));
    let (status, accepted) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let retained: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contact_method WHERE person_id=$1 AND octet_length(value)>8000",
    )
    .bind(f.person)
    .fetch_one(&f.app)
    .await
    .unwrap();
    assert_eq!(retained, 151);
    let (status, conflict) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &format!("{base}?cursor={}", first_cursor.unwrap()),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"], "revision_conflict");
    // One unrepresentable row is an explicit bounded error, not truncation.
    sqlx::query("UPDATE contact_method SET value=repeat('x',525000) WHERE id=$1")
        .bind(expected[0])
        .execute(&f.app)
        .await
        .unwrap();
    let (status, oversized) = request(
        &f.router,
        &f.cookie,
        Some(f.context),
        "GET",
        &base,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(oversized["error"], "over_limit");
}

#[sqlx::test]
#[ignore]
async fn mobile005_profile_rejects_foreign_ids_and_keeps_unrelated_mutations_independent(
    pool: PgPool,
) {
    let f = fixture(&pool).await;
    let (foreign_org, _) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Profile Foreign",
        "profile-foreign@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    let foreign:Uuid=sqlx::query_scalar("INSERT INTO person(organization_id,stage_id,first_name) SELECT $1,id,'Foreign' FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1 RETURNING id")
        .bind(foreign_org).fetch_one(&pool).await.unwrap();
    let contact:Uuid=sqlx::query_scalar("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) VALUES($1,$2,'email','household@example.test','household@example.test') RETURNING id")
        .bind(foreign_org).bind(foreign).fetch_one(&pool).await.unwrap();
    let household_person:Uuid=sqlx::query_scalar("INSERT INTO person(organization_id,stage_id,first_name) SELECT $1,stage_id,'Same household' FROM person WHERE id=$2 RETURNING id")
        .bind(f.org).bind(f.person).fetch_one(&f.app).await.unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) VALUES($1,$2,'email','household@example.test','household@example.test')")
        .bind(f.org).bind(household_person).execute(&f.app).await.unwrap();
    for target in [foreign, Uuid::new_v4()] {
        let path = format!("/api/mobile/v1/people/{target}/details");
        assert_eq!(
            request(
                &f.router,
                &f.cookie,
                Some(f.context),
                "GET",
                &path,
                json!(null)
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        let operation=f.operation("update_person_details",json!({"person_id":target,"expected_details_revision":"1","first_name":"Forbidden","contact_operations":[]}));
        assert_eq!(
            f.post("/api/mobile/v1/operations", operation).await.0,
            StatusCode::NOT_FOUND
        );
    }
    let foreign_contact=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":"1","contact_operations":[{"op":"remove","id":contact}]}));
    assert_eq!(
        f.post("/api/mobile/v1/operations", foreign_contact).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let path = format!("/api/mobile/v1/people/{}/details", f.person);
    assert_eq!(
        request(&f.router, "", Some(f.context), "GET", &path, json!(null))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // Ordinary note/task and assignment changes alter the broad generation
    // revision, but do not stale a pending details baseline.
    sqlx::query("INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id) VALUES($1,$2,$3,'Unrelated note','web_session',gen_random_uuid())")
        .bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("INSERT INTO task(organization_id,person_id,title,kind,created_by_user_id,assignee_user_id,origin,correlation_id) VALUES($1,$2,'Unrelated task','follow_up',$3,$3,'web_session',gen_random_uuid())")
        .bind(f.org).bind(f.person).bind(f.actor).execute(&f.app).await.unwrap();
    sqlx::query("UPDATE person SET assigned_user_id=$2 WHERE id=$1")
        .bind(f.person)
        .bind(f.other)
        .execute(&f.app)
        .await
        .unwrap();
    let tag:Uuid=sqlx::query_scalar("INSERT INTO tag(organization_id,created_by_user_id,name) VALUES($1,$2,'Unrelated tag') RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) VALUES($1,$2,$3,$4)")
        .bind(f.org)
        .bind(f.person)
        .bind(tag)
        .bind(f.actor)
        .execute(&f.app)
        .await
        .unwrap();
    let field:Uuid=sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) VALUES($1,'Unrelated field','text',1,$2) RETURNING id")
        .bind(f.org).bind(f.actor).fetch_one(&f.app).await.unwrap();
    sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) VALUES($1,$2,$3,'text','Unrelated',$4,'web_session',gen_random_uuid())")
        .bind(f.org).bind(f.person).bind(field).bind(f.actor).execute(&f.app).await.unwrap();
    let (details, broad): (i64, i64) =
        sqlx::query_as("SELECT details_revision,mobile_revision FROM person WHERE id=$1")
            .bind(f.person)
            .fetch_one(&f.app)
            .await
            .unwrap();
    assert_eq!(details, 1);
    assert!(broad > 1);
    let household=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":"1","contact_operations":[{"op":"add","kind":"email","value":"household@example.test"}]}));
    let (status, receipt) = f.post("/api/mobile/v1/operations", household).await;
    assert_eq!(status, StatusCode::OK, "{receipt}");
    let shared:i64=sqlx::query_scalar("SELECT count(*) FROM contact_method WHERE organization_id=$1 AND normalized_value='household@example.test'")
        .bind(f.org).fetch_one(&f.app).await.unwrap();
    assert_eq!(shared, 2);
    let foreign_unchanged: String =
        sqlx::query_scalar("SELECT value FROM contact_method WHERE id=$1")
            .bind(contact)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(foreign_unchanged, "household@example.test");
}

#[sqlx::test]
#[ignore]
async fn mobile005_publish_failure_keeps_committed_receipt_and_intake_admission_precedes_person_lock(
    pool: PgPool,
) {
    use crm_api::realtime::CentrifugoTransport;
    use std::sync::Arc;
    use std::time::Duration;
    let mut f = fixture(&pool).await;
    let notified = Arc::new(tokio::sync::Notify::new());
    let signal = notified.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fake = Router::new().route(
        "/publish",
        axum::routing::post(move || {
            let signal = signal.clone();
            async move {
                signal.notify_one();
                StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });
    let publisher = Publisher::Centrifugo(CentrifugoTransport::for_tests(
        format!("http://{address}"),
        "synthetic-only",
    ));
    f.router = crate::common::build_router_with_publisher(&pool, publisher).await;
    let operation=f.operation("update_person_details",json!({"person_id":f.person,"expected_details_revision":"1","first_name":"Durable despite publisher","contact_operations":[]}));
    let mut blocked = f.app.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('intake:' || $1::text,0))")
        .bind(f.org.to_string())
        .execute(&mut *blocked)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM person WHERE id=$1 FOR UPDATE")
        .bind(f.person)
        .execute(&mut *blocked)
        .await
        .unwrap();
    let (status, busy) = tokio::time::timeout(
        Duration::from_secs(1),
        f.post("/api/mobile/v1/operations", operation.clone()),
    )
    .await
    .expect("profile admission must not wait for the Person lock while intake is held");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(busy["error"], "intake_busy");
    blocked.rollback().await.unwrap();
    let (status, accepted) = f.post("/api/mobile/v1/operations", operation.clone()).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["changed"], true);
    tokio::time::timeout(Duration::from_secs(3), notified.notified())
        .await
        .expect("failed publish was actually attempted");
    let (status, replayed) = f.post("/api/mobile/v1/operations", operation).await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed["replayed"], true);
    assert_eq!(
        replayed["committed_revision"],
        accepted["committed_revision"]
    );
    let (name,count):(String,i64)=sqlx::query_as("SELECT first_name,(SELECT count(*) FROM mobile_operation_receipt WHERE person_id=$1) FROM person WHERE id=$1")
        .bind(f.person).fetch_one(&f.app).await.unwrap();
    assert_eq!(name, "Durable despite publisher");
    assert_eq!(count, 1);
    server.abort();
}

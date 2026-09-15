//! M7-02/03: real router, app role, isolation and transient discovery contract.
use super::*;
const PATH: &str = "/api/mobile/v1/people/search";

#[sqlx::test]
#[ignore]
async fn mobile007_search_matching_bounds_and_no_mutation(pool: PgPool) {
    let f = fixture(&pool).await;
    assert!(f.bootstrap["capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("people_search")));
    sqlx::query(
        "UPDATE person SET first_name='Ada',last_name='Lovelace',assigned_user_id=$2 WHERE id=$1",
    )
    .bind(f.person)
    .bind(f.other)
    .execute(&f.app)
    .await
    .unwrap();
    sqlx::query("INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value) VALUES($1,$2,'email','Ada@Example.test','ada@example.test'),($1,$2,'phone','(415) 555-0100','+14155550100')").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let mut connection = f.app.acquire().await.unwrap();
    let scope = crm_api::domain::person::PersonVisibilityScope::Organization(
        crm_api::ids::OrganizationId::new(f.org),
    );
    let (hits, more) = crm_api::domain::person::discovery::search(&mut connection, &scope, "ada")
        .await
        .expect("typed discovery query");
    assert_eq!(hits.len(), 1);
    assert!(!more);
    drop(connection);
    let before: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM mobile_context c WHERE id=$1")
        .bind(f.context)
        .fetch_one(&pool)
        .await
        .unwrap();
    for term in [
        "ada",
        "Lovelace",
        " Ada Lovelace ",
        "ADA@example.test",
        "4155550100",
    ] {
        let (status, result) = f.post(PATH, json!({"term":term})).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["context_id"], json!(f.context));
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        let item = &result["items"][0];
        assert_eq!(item["person_id"], json!(f.person));
        assert_eq!(item["display_name"], "Ada Lovelace");
        assert_eq!(item["assigned_user"]["id"], json!(f.other));
        assert_eq!(item["primary_email"], "Ada@Example.test");
        assert_eq!(item.as_object().unwrap().len(), 6);
        assert_eq!(result["has_more"], false);
    }
    for term in ["example.test", "%", "_", "\\", "1415555", "No such person"] {
        let (status, result) = f.post(PATH, json!({"term":term})).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["items"], json!([]), "literal/exact match: {term}");
    }
    let after: Value = sqlx::query_scalar("SELECT to_jsonb(c) FROM mobile_context c WHERE id=$1")
        .bind(f.context)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "search never renews access or changes context"
    );
    for table in ["mobile_operation_receipt", "mobile_reconciliation"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE organization_id=$1"
        ))
        .bind(f.org)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "search must not write {table}");
    }
    // Same names remain separate IDs, with a deterministic ID tie-break.
    sqlx::query("INSERT INTO person(organization_id,first_name,last_name,stage_id) SELECT $1,'Ada','Lovelace',stage_id FROM person p CROSS JOIN generate_series(1,24) n WHERE p.id=$2").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let (_, exact_limit) = f.post(PATH, json!({"term":"Ada"})).await;
    assert_eq!(exact_limit["items"].as_array().unwrap().len(), 25);
    assert_eq!(exact_limit["has_more"], false);
    assert!(exact_limit["items"].as_array().unwrap().iter().any(|item| {
        item["assigned_user"].is_null()
            && item["primary_email"].is_null()
            && item["primary_phone"].is_null()
    }));
    sqlx::query("INSERT INTO person(organization_id,first_name,last_name,stage_id) SELECT $1,'Ada','Lovelace',stage_id FROM person WHERE id=$2").bind(f.org).bind(f.person).execute(&f.app).await.unwrap();
    let (_, result) = f.post(PATH, json!({"term":"Ada"})).await;
    assert_eq!(result["items"].as_array().unwrap().len(), 25);
    assert_eq!(result["has_more"], true);
    let ids: Vec<Uuid> = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| id(i, "person_id"))
        .collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(serde_json::to_vec(&result).unwrap().len() <= 128 * 1024);
    let (_, repeat) = f.post(PATH, json!({"term":"Ada"})).await;
    assert_eq!(result, repeat);

    // A nameless Person is discoverable by an exact contact and uses the same
    // display-name fallback as ordinary Person readers.
    sqlx::query("UPDATE person SET first_name=NULL,last_name=NULL WHERE id=$1")
        .bind(f.person)
        .execute(&f.app)
        .await
        .unwrap();
    let (status, nameless) = f.post(PATH, json!({"term":"ada@example.test"})).await;
    assert_eq!(status, StatusCode::OK, "{nameless}");
    assert_eq!(nameless["items"][0]["display_name"], "Ada@Example.test");

    // Stored legacy values can exceed ordinary write validation. The API must
    // fail the complete response instead of publishing an oversized preview.
    sqlx::query("UPDATE person SET first_name=$2 WHERE id=$1")
        .bind(f.person)
        .bind("Oversized".repeat(600))
        .execute(&f.app)
        .await
        .unwrap();
    let (status, oversized) = f.post(PATH, json!({"term":"Oversized"})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{oversized}");
    assert_eq!(oversized["error"], "unavailable");
    assert!(oversized.get("items").is_none());
}

#[sqlx::test]
#[ignore]
async fn mobile007_search_shape_authority_and_held_workspace(pool: PgPool) {
    let f = fixture(&pool).await;
    for input in [
        json!({}),
        json!({"term":1}),
        json!({"term":null}),
        json!({"term":"Ada","organization_id":f.org}),
        json!([]),
    ] {
        let (status, body) = f.post(PATH, input).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"], "malformed_request");
    }
    for term in [
        "".to_string(),
        " \t\n".to_string(),
        "x".repeat(201),
        "😀".repeat(201),
    ] {
        let (status, body) = f.post(PATH, json!({"term":term})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"], "invalid_input");
    }
    assert_eq!(
        f.post(PATH, json!({"term":"😀".repeat(200)})).await.0,
        StatusCode::OK
    );
    assert_eq!(
        f.post(&format!("{PATH}?term=Ada"), json!({"term":"Ada"}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.post(PATH, json!({"term":"x".repeat(4096)})).await.0,
        StatusCode::BAD_REQUEST
    );
    for context in [None, Some(Uuid::new_v4())] {
        assert_eq!(
            request(
                &f.router,
                &f.cookie,
                context,
                "POST",
                PATH,
                json!({"term":"Synthetic"})
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (foreign_org, _) = crate::common::create_org_with_stages_and_member(
        &pool,
        "Foreign search",
        "foreign-search@fixture.test",
        "Foreign",
        PW,
    )
    .await;
    sqlx::query("INSERT INTO person(organization_id,first_name,stage_id) SELECT $1,'ForeignSecret',id FROM stage WHERE organization_id=$1 LIMIT 1").bind(foreign_org).execute(&f.app).await.unwrap();
    assert_eq!(
        f.post(PATH, json!({"term":"ForeignSecret"})).await.1["items"],
        json!([])
    );
    let cookie = crate::common::login_cookie(&f.router, "foreign-search@fixture.test", PW).await;
    assert_eq!(
        request(
            &f.router,
            &cookie,
            Some(f.context),
            "POST",
            PATH,
            json!({"term":"Synthetic"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query(
        "UPDATE mobile_context SET offline_access_expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(f.context)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        f.post(PATH, json!({"term":"Synthetic"})).await.0,
        StatusCode::UNAUTHORIZED
    );
    request(
        &f.router,
        &f.cookie,
        None,
        "POST",
        "/api/mobile/v1/bootstrap",
        json!({"protocol":"mobile-v1","installation_id":f.install}),
    )
    .await;
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    assert!(!f
        .post(PATH, json!({"term":"Synthetic"}))
        .await
        .0
        .is_success());
    sqlx::query("UPDATE organization_membership SET status='active',role='admin' WHERE organization_id=$1 AND user_id=$2").bind(f.org).bind(f.actor).execute(&pool).await.unwrap();
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1").bind(f.org).execute(&pool).await.unwrap();
    let (status, body) = f.post(PATH, json!({"term":"Synthetic"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"], "workspace_in_migration_review");
}

//! DB-backed schema tests for Slice 002 (docs/specs/SLICE_002.md §13,
//! acceptance criteria 1–2): `crm_app` grants exactly as specified, and the
//! append-only trigger on each fact table. Run only via ./scripts/check-db.

use sqlx::PgPool;
use uuid::Uuid;

/// Criterion 1: `crm_app` grants are exactly spec §2, table by table.
#[sqlx::test]
#[ignore]
async fn crm_app_has_exactly_the_slice_002_grants(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    // `stage`: amended by docs/specs/SLICE_004.md §2 (declared change,
    // AGENTS.md §11) — `crm_app` gains INSERT (`stage::seed_defaults` moves
    // to the application path via `CreateOrganization`), so this is no
    // longer the SELECT-only table this comment described under Slice 002.
    // No UPDATE/DELETE grant either way. The bare `INSERT ... DEFAULT
    // VALUES` pattern the loop below still uses for other SELECT-only
    // tables would `.is_err()` here for the wrong reason (a NOT NULL
    // violation, not a permission denial) now that INSERT is granted, so
    // `stage` is tested on its own with a real, FK-satisfying row —
    // positive proof the grant actually works, not just that some INSERT
    // failed.
    let select = sqlx::query("SELECT * FROM stage")
        .fetch_all(&app_pool)
        .await;
    assert!(select.is_ok(), "stage: SELECT must succeed for crm_app");

    let org_id = crate::common::create_org(&migrator_pool, "Grant Check Realty").await;
    let stage_insert = sqlx::query(
        "INSERT INTO stage (organization_id, name, position) VALUES ($1, 'Custom Stage', 99)",
    )
    .bind(org_id)
    .execute(&app_pool)
    .await;
    assert!(
        stage_insert.is_ok(),
        "stage: INSERT must succeed for crm_app (SLICE_004 §2)"
    );

    let stage_update = sqlx::query("UPDATE stage SET name = name WHERE false")
        .execute(&app_pool)
        .await;
    assert!(
        stage_update.is_err(),
        "stage: UPDATE must be denied for crm_app"
    );

    let stage_delete = sqlx::query("DELETE FROM stage").execute(&app_pool).await;
    assert!(
        stage_delete.is_err(),
        "stage: DELETE must be denied for crm_app"
    );

    // `contact_method`, `inquiry`, and the fact tables (the five from
    // Slices 002/003 plus `call_completed`, docs/specs/SLICE_006.md §2):
    // SELECT + INSERT, no UPDATE/DELETE.
    for table in [
        "contact_method",
        "inquiry",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "contact_attempted",
        "call_completed",
    ] {
        let select = sqlx::query(&format!("SELECT * FROM {table}"))
            .fetch_all(&app_pool)
            .await;
        assert!(select.is_ok(), "{table}: SELECT must succeed for crm_app");

        let insert = sqlx::query(&format!("INSERT INTO {table} DEFAULT VALUES"))
            .execute(&app_pool)
            .await;
        assert!(
            insert.is_err(),
            "{table}: bare INSERT must fail on required columns, not permission — \
             but a permission grant is what makes the *attempt* meaningful; the table's \
             actual INSERT grant is exercised positively elsewhere in this suite"
        );

        let update = sqlx::query(&format!("UPDATE {table} SET id = id WHERE false"))
            .execute(&app_pool)
            .await;
        assert!(
            update.is_err(),
            "{table}: UPDATE must be denied for crm_app"
        );

        let delete = sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&app_pool)
            .await;
        assert!(
            delete.is_err(),
            "{table}: DELETE must be denied for crm_app"
        );
    }

    // `person`: SELECT + INSERT + UPDATE, no DELETE.
    let select = sqlx::query("SELECT * FROM person")
        .fetch_all(&app_pool)
        .await;
    assert!(select.is_ok(), "person: SELECT must succeed for crm_app");
    let update = sqlx::query("UPDATE person SET first_name = first_name WHERE false")
        .execute(&app_pool)
        .await;
    assert!(update.is_ok(), "person: UPDATE must succeed for crm_app");
    let delete = sqlx::query("DELETE FROM person").execute(&app_pool).await;
    assert!(delete.is_err(), "person: DELETE must be denied for crm_app");

    // `raw_payload`: SELECT + INSERT, and column-level UPDATE only on
    // (resolution, unresolved_reason, resolved_at, inquiry_id) — nonce,
    // ciphertext, and content_hmac stay immutable to the application.
    let select = sqlx::query("SELECT * FROM raw_payload")
        .fetch_all(&app_pool)
        .await;
    assert!(
        select.is_ok(),
        "raw_payload: SELECT must succeed for crm_app"
    );

    let allowed_update = sqlx::query("UPDATE raw_payload SET resolution = resolution WHERE false")
        .execute(&app_pool)
        .await;
    assert!(
        allowed_update.is_ok(),
        "raw_payload: UPDATE on the granted columns must succeed for crm_app"
    );

    let denied_update = sqlx::query("UPDATE raw_payload SET ciphertext = ciphertext WHERE false")
        .execute(&app_pool)
        .await;
    assert!(
        denied_update.is_err(),
        "raw_payload: UPDATE on ciphertext must be denied for crm_app"
    );
    let denied_update_nonce = sqlx::query("UPDATE raw_payload SET nonce = nonce WHERE false")
        .execute(&app_pool)
        .await;
    assert!(
        denied_update_nonce.is_err(),
        "raw_payload: UPDATE on nonce must be denied for crm_app"
    );
    let denied_update_hmac =
        sqlx::query("UPDATE raw_payload SET content_hmac = content_hmac WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        denied_update_hmac.is_err(),
        "raw_payload: UPDATE on content_hmac must be denied for crm_app"
    );

    let delete = sqlx::query("DELETE FROM raw_payload")
        .execute(&app_pool)
        .await;
    assert!(
        delete.is_err(),
        "raw_payload: DELETE must be denied for crm_app"
    );
}

/// Positive proof that the tables granted INSERT actually accept one,
/// scoped to real FK-satisfying fixture rows (the generic loop above only
/// proves a *bare* `DEFAULT VALUES` insert is rejected, which conflates
/// "denied by grant" with "denied by NOT NULL" — this closes that gap).
#[sqlx::test]
#[ignore]
async fn crm_app_can_actually_write_the_granted_tables(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let (stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let person_insert =
        sqlx::query("INSERT INTO person (organization_id, stage_id) VALUES ($1, $2)")
            .bind(org_id)
            .bind(stage_id)
            .execute(&app_pool)
            .await;
    assert!(
        person_insert.is_ok(),
        "crm_app must be able to INSERT person"
    );

    let raw_payload_insert = sqlx::query(
        r#"INSERT INTO raw_payload
            (id, organization_id, source, payload_format, origin, received_at,
             nonce, ciphertext, content_hmac, byte_len, resolution)
           VALUES ($1, $2, 'zillow', 'generic_v1', 'web_session', now(), $3, $4, $5, 10, 'pending')"#,
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(vec![0u8; 24])
    .bind(vec![0u8; 26])
    .bind(vec![0u8; 32])
    .execute(&app_pool)
    .await;
    assert!(
        raw_payload_insert.is_ok(),
        "crm_app must be able to INSERT raw_payload"
    );
}

struct FactRowIds {
    inquiry_received_id: Uuid,
    routing_decision_id: Uuid,
    assignment_changed_id: Uuid,
    stage_changed_id: Uuid,
    contact_attempted_id: Uuid,
    call_completed_id: Uuid,
}

async fn insert_one_row_per_fact_table(
    migrator_pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
    stage_id: Uuid,
) -> FactRowIds {
    let correlation_id = Uuid::new_v4();

    let (inquiry_received_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO inquiry_received
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             inquiry_id, person_id, raw_payload_id, content_hmac, source, person_created, matched_by)
           VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, $5, $6, $7, 'zillow', true, NULL)
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(vec![0u8; 32])
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    let (routing_decision_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO routing_decision
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             inquiry_id, person_id, strategy, assignee_user_id)
           VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, $5, 'actor_default', $2)
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    let (assignment_changed_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO assignment_changed
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, from_user_id, to_user_id, reason)
           VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, NULL, $2, 'intake')
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
    .bind(Uuid::new_v4())
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    let (stage_changed_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO stage_changed
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, from_stage_id, to_stage_id, reason)
           VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, NULL, $5, 'intake')
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
    .bind(Uuid::new_v4())
    .bind(stage_id)
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    let (contact_attempted_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO contact_attempted
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             person_id, channel, outcome)
           VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, 'call', 'no_answer')
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
    .bind(Uuid::new_v4())
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    let (call_completed_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO call_completed
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             call_id, person_id, contact_method_id, outcome, answered_at, ended_at, talk_seconds)
           VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, $5, $6, 'reached', now(), now(), 0)
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .fetch_one(migrator_pool)
    .await
    .unwrap();

    FactRowIds {
        inquiry_received_id,
        routing_decision_id,
        assignment_changed_id,
        stage_changed_id,
        contact_attempted_id,
        call_completed_id,
    }
}

/// Criterion 2: UPDATE and DELETE on each fact table fail both as `crm_app`
/// (the grant) and as `crm_migrator` (the trigger — a `FOR EACH ROW`
/// trigger never fires against zero matching rows, so this must target an
/// *existing* row, unlike criterion 1's `WHERE false` pattern).
#[sqlx::test]
#[ignore]
async fn fact_tables_are_append_only_via_grant_and_trigger(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let user_id =
        crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, user_id).await;
    let (stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    let rows = insert_one_row_per_fact_table(&migrator_pool, org_id, user_id, stage_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let cases: [(&str, Uuid); 6] = [
        ("inquiry_received", rows.inquiry_received_id),
        ("routing_decision", rows.routing_decision_id),
        ("assignment_changed", rows.assignment_changed_id),
        ("stage_changed", rows.stage_changed_id),
        ("contact_attempted", rows.contact_attempted_id),
        ("call_completed", rows.call_completed_id),
    ];

    for (table, id) in cases {
        // As crm_app: denied by the grant (no UPDATE/DELETE privilege).
        // `occurred_at` exists on every fact table, unlike `reason`.
        let app_update_generic = sqlx::query(&format!(
            "UPDATE {table} SET occurred_at = occurred_at WHERE id = $1"
        ))
        .bind(id)
        .execute(&app_pool)
        .await;
        assert!(
            app_update_generic.is_err(),
            "{table}: crm_app UPDATE must be denied (grant)"
        );

        let app_delete = sqlx::query(&format!("DELETE FROM {table} WHERE id = $1"))
            .bind(id)
            .execute(&app_pool)
            .await;
        assert!(
            app_delete.is_err(),
            "{table}: crm_app DELETE must be denied (grant)"
        );

        // As crm_migrator: has full DML privilege as schema owner, so a
        // denial here can only come from the append-only trigger.
        let migrator_update = sqlx::query(&format!(
            "UPDATE {table} SET occurred_at = occurred_at WHERE id = $1"
        ))
        .bind(id)
        .execute(&migrator_pool)
        .await;
        assert!(
            migrator_update.is_err(),
            "{table}: crm_migrator UPDATE must be denied by the append-only trigger"
        );

        let migrator_delete = sqlx::query(&format!("DELETE FROM {table} WHERE id = $1"))
            .bind(id)
            .execute(&migrator_pool)
            .await;
        assert!(
            migrator_delete.is_err(),
            "{table}: crm_migrator DELETE must be denied by the append-only trigger"
        );
    }
}

/// TRUNCATE is a third mutation path distinct from UPDATE/DELETE: Postgres
/// row-level triggers (the ones criterion 2 relies on) never fire on
/// TRUNCATE, so a `BEFORE UPDATE OR DELETE FOR EACH ROW` trigger alone
/// leaves TRUNCATE unblocked for any role that holds (or, as table owner,
/// implicitly has) the TRUNCATE privilege — `crm_migrator` in particular.
/// Each fact table also has a `BEFORE TRUNCATE FOR EACH STATEMENT`
/// trigger, on the same `reject_mutation()` function, closing that gap.
#[sqlx::test]
#[ignore]
async fn fact_tables_reject_truncate_via_grant_and_trigger(migrator_pool: PgPool) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    crate::common::seed_stages(&migrator_pool, org_id).await;
    let user_id =
        crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, user_id).await;
    let (stage_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
            .bind(org_id)
            .fetch_one(&migrator_pool)
            .await
            .unwrap();

    // A row per table, so a TRUNCATE that *did* succeed would be a
    // detectable, non-vacuous data loss, not just a permission probe
    // against an empty table.
    insert_one_row_per_fact_table(&migrator_pool, org_id, user_id, stage_id).await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let tables = [
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "contact_attempted",
        "call_completed",
    ];

    for table in tables {
        // As crm_app: denied — crm_app was never granted TRUNCATE on any
        // table.
        let app_truncate = sqlx::query(&format!("TRUNCATE TABLE {table}"))
            .execute(&app_pool)
            .await;
        assert!(
            app_truncate.is_err(),
            "{table}: crm_app TRUNCATE must be denied"
        );

        // As crm_migrator: schema owner, so implicitly holds TRUNCATE —
        // this is exactly the case the new statement-level trigger exists
        // to block. Without it, this would silently succeed.
        let migrator_truncate = sqlx::query(&format!("TRUNCATE TABLE {table}"))
            .execute(&migrator_pool)
            .await;
        assert!(
            migrator_truncate.is_err(),
            "{table}: crm_migrator TRUNCATE must be denied by the append-only trigger"
        );

        let (count,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&migrator_pool)
            .await
            .unwrap();
        assert_eq!(
            count, 1,
            "{table}: the fixture row must survive both TRUNCATE attempts"
        );
    }
}

/// docs/specs/SLICE_006c.md §2, §13: the widened `outcome` CHECK and the
/// linear-chain partial unique index `contact_attempted_corrects_once`.
#[sqlx::test]
#[ignore]
async fn contact_attempted_outcome_check_and_corrects_once_index_are_section_2(
    migrator_pool: PgPool,
) {
    let org_id = crate::common::create_org(&migrator_pool, "Acme Realty").await;
    let user_id =
        crate::common::create_user(&migrator_pool, "alice@acme.test", "Alice", "pw").await;
    crate::common::add_membership(&migrator_pool, org_id, user_id).await;
    let person_id = Uuid::new_v4();

    let insert = |outcome: &'static str, corrects_id: Option<Uuid>| {
        let migrator_pool = migrator_pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                r#"INSERT INTO contact_attempted
                    (organization_id, actor_kind, actor_user_id, origin, occurred_at,
                     correlation_id, person_id, channel, outcome, corrects_id)
                   VALUES ($1, 'user', $2, 'web_session', now(), $3, $4, 'call', $5, $6)
                   RETURNING id"#,
            )
            .bind(org_id)
            .bind(user_id)
            .bind(Uuid::new_v4())
            .bind(person_id)
            .bind(outcome)
            .bind(corrects_id)
            .fetch_one(&migrator_pool)
            .await
        }
    };

    // The CHECK accepts exactly the six values.
    let mut ids = Vec::new();
    for outcome in [
        "reached",
        "no_answer",
        "left_message",
        "sent",
        "busy",
        "wrong_number",
    ] {
        ids.push(
            insert(outcome, None)
                .await
                .unwrap_or_else(|e| panic!("{outcome}: {e}")),
        );
    }
    for outcome in ["voicemail", "answered", "declined", ""] {
        let err = insert(outcome, None).await.unwrap_err();
        let db = err.as_database_error().expect("a CHECK violation");
        assert_eq!(db.code().as_deref(), Some("23514"), "{outcome}");
        assert_eq!(db.constraint(), Some("contact_attempted_outcome_check"));
    }

    // The partial unique index: a row is corrected at most once; NULLs
    // are unconstrained (the six originals above already prove that).
    let (indexdef,): (String,) = sqlx::query_as(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'contact_attempted_corrects_once'",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    assert!(indexdef.contains("UNIQUE"), "{indexdef}");
    assert!(indexdef.contains("WHERE"), "{indexdef}");
    assert!(indexdef.contains("corrects_id IS NOT NULL"), "{indexdef}");
    let head = ids[0];
    let first = insert("busy", Some(head)).await.unwrap();
    let err = insert("wrong_number", Some(head)).await.unwrap_err();
    let db = err.as_database_error().expect("a unique violation");
    assert_eq!(db.code().as_deref(), Some("23505"));
    assert_eq!(db.constraint(), Some("contact_attempted_corrects_once"));
    // Chaining onto the correction is fine.
    insert("wrong_number", Some(first)).await.unwrap();
    // A correction row is append-only like every other fact row.
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    for pool in [&app_pool, &migrator_pool] {
        assert!(
            sqlx::query("UPDATE contact_attempted SET corrects_id = NULL WHERE id = $1")
                .bind(first)
                .execute(pool)
                .await
                .is_err()
        );
        assert!(sqlx::query("DELETE FROM contact_attempted WHERE id = $1")
            .bind(first)
            .execute(pool)
            .await
            .is_err());
    }
}

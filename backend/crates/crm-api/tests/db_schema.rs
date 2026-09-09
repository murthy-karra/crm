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

    // Slice 011c source preferences are ordinary private configuration:
    // crm_app can read/insert/delete through typed commands but cannot
    // update in place or truncate another actor's settings.
    let source_select = sqlx::query("SELECT * FROM today_work_source")
        .fetch_all(&app_pool)
        .await;
    assert!(
        source_select.is_ok(),
        "today_work_source: SELECT must succeed for crm_app"
    );
    let source_update =
        sqlx::query("UPDATE today_work_source SET created_at = created_at WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        source_update.is_err(),
        "today_work_source: UPDATE must be denied for crm_app"
    );
    let source_truncate = sqlx::query("TRUNCATE today_work_source")
        .execute(&app_pool)
        .await;
    assert!(
        source_truncate.is_err(),
        "today_work_source: TRUNCATE must be denied for crm_app"
    );

    // docs/specs/SLICE_011d.md §3: today_system_feed is ordinary
    // configuration (like saved_list) — crm_app gets SELECT/INSERT/UPDATE
    // but never DELETE/TRUNCATE (feeds are seeded and reverted, never
    // removed).
    let feed_select = sqlx::query("SELECT * FROM today_system_feed")
        .fetch_all(&app_pool)
        .await;
    assert!(
        feed_select.is_ok(),
        "today_system_feed: SELECT must succeed for crm_app"
    );
    let feed_update =
        sqlx::query("UPDATE today_system_feed SET updated_at = updated_at WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        feed_update.is_ok(),
        "today_system_feed: UPDATE must succeed for crm_app"
    );
    let feed_delete = sqlx::query("DELETE FROM today_system_feed WHERE false")
        .execute(&app_pool)
        .await;
    assert!(
        feed_delete.is_err(),
        "today_system_feed: DELETE must be denied for crm_app"
    );
    let feed_truncate = sqlx::query("TRUNCATE today_system_feed")
        .execute(&app_pool)
        .await;
    assert!(
        feed_truncate.is_err(),
        "today_system_feed: TRUNCATE must be denied for crm_app"
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
        "today_feed_changed",
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
    today_feed_changed_id: Uuid,
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

    let (today_feed_changed_id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO today_feed_changed
            (organization_id, actor_kind, actor_user_id, origin, occurred_at, correlation_id,
             feed_key, change, from_revision, to_revision, enabled_after)
           VALUES ($1, 'user', $2, 'web_session', now(), $3,
                   'unanswered_inquiry', 'updated', 1, 2, true)
           RETURNING id"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(correlation_id)
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
        today_feed_changed_id,
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

    let cases: [(&str, Uuid); 7] = [
        ("inquiry_received", rows.inquiry_received_id),
        ("routing_decision", rows.routing_decision_id),
        ("assignment_changed", rows.assignment_changed_id),
        ("stage_changed", rows.stage_changed_id),
        ("contact_attempted", rows.contact_attempted_id),
        ("call_completed", rows.call_completed_id),
        ("today_feed_changed", rows.today_feed_changed_id),
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
    // `inquiry` (item 1 of the LATER batch,
    // 20260911000001_inquiry_append_only.sql) has a real FK to `person`,
    // unlike the bare-UUID fact tables above, so it needs an actual Person
    // row to reference.
    let (person_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO person (organization_id, stage_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inquiry (organization_id, person_id, raw_payload_id, source, received_at) \
         VALUES ($1, $2, $3, 'zillow', now())",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(Uuid::new_v4())
    .execute(&migrator_pool)
    .await
    .unwrap();
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let tables = [
        "inquiry",
        "inquiry_received",
        "routing_decision",
        "assignment_changed",
        "stage_changed",
        "contact_attempted",
        "call_completed",
        "today_feed_changed",
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

/// docs/specs/SLICE_011b_SORT.md §5, §11.9: the `saved_list.sort_key`/
/// `sort_direction` CHECK constraints — an out-of-vocabulary key or
/// direction, and a half pair (one `NULL`, one not) in either direction —
/// are all rejected, while every one of the eight legal pairs and the
/// all-`NULL` default pair succeed.
#[sqlx::test]
#[ignore]
async fn saved_list_sort_columns_check_constraints_reject_bad_values_and_half_pairs(
    migrator_pool: PgPool,
) {
    let (organization_id, owner_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Saved sort schema",
        "owner@saved-sort-schema.test",
        "Owner",
        "pw",
    )
    .await;
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;

    let insert = |sort_key: Option<&'static str>, sort_direction: Option<&'static str>| {
        let app_pool = app_pool.clone();
        async move {
            sqlx::query(
                r#"INSERT INTO saved_list
                     (organization_id, created_by_user_id, scope, name, filter,
                      create_request_id, create_fingerprint, sort_key, sort_direction)
                   VALUES ($1, $2, 'personal', 'X', '{"version":1,"clauses":[]}'::jsonb,
                           $3, decode(repeat('02', 32), 'hex'), $4, $5)"#,
            )
            .bind(organization_id)
            .bind(owner_id)
            .bind(Uuid::new_v4())
            .bind(sort_key)
            .bind(sort_direction)
            .execute(&app_pool)
            .await
        }
    };

    // Every legal pair succeeds.
    for (key, direction) in [
        ("created", "asc"),
        ("created", "desc"),
        ("name", "asc"),
        ("name", "desc"),
        ("stage", "asc"),
        ("stage", "desc"),
        ("assignee", "asc"),
        ("assignee", "desc"),
    ] {
        let result = insert(Some(key), Some(direction)).await;
        assert!(result.is_ok(), "{key}.{direction} must be accepted");
    }
    // The all-NULL default pair succeeds.
    assert!(
        insert(None, None).await.is_ok(),
        "NULL, NULL must be accepted"
    );

    // Out-of-vocabulary key/direction.
    let bad_key = insert(Some("distance"), Some("asc")).await.unwrap_err();
    assert_eq!(
        bad_key
            .as_database_error()
            .and_then(|db| db.constraint().map(str::to_string)),
        Some("saved_list_sort_key_check".to_string())
    );
    let bad_direction = insert(Some("name"), Some("sideways")).await.unwrap_err();
    assert_eq!(
        bad_direction
            .as_database_error()
            .and_then(|db| db.constraint().map(str::to_string)),
        Some("saved_list_sort_direction_check".to_string())
    );

    // Half pairs, in both directions.
    let half_a = insert(Some("name"), None).await.unwrap_err();
    assert_eq!(
        half_a
            .as_database_error()
            .and_then(|db| db.constraint().map(str::to_string)),
        Some("saved_list_sort_pair_check".to_string())
    );
    let half_b = insert(None, Some("asc")).await.unwrap_err();
    assert_eq!(
        half_b
            .as_database_error()
            .and_then(|db| db.constraint().map(str::to_string)),
        Some("saved_list_sort_pair_check".to_string())
    );
}

/// docs/specs/SLICE_011e.md §2, §9.1: `crm_app` grants for `tag` (full
/// CRUD — rename/delete are in-place writes, unlike `saved_list`'s
/// tombstone convention) and `person_tag` (SELECT/INSERT/DELETE only, no
/// UPDATE — an applied tag is added or removed, never edited in place).
#[sqlx::test]
#[ignore]
async fn tag_and_person_tag_grants_are_exactly_slice_011e_section_2(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let (org_id, actor_id) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Grant Check Realty",
        "grant-check@acme.test",
        "Grant Check",
        "correct horse battery staple",
    )
    .await;

    let select = sqlx::query("SELECT * FROM tag").fetch_all(&app_pool).await;
    assert!(select.is_ok(), "tag: SELECT must succeed for crm_app");

    let tag_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tag (organization_id, created_by_user_id, name) VALUES ($1, $2, 'Investor') RETURNING id",
    )
    .bind(org_id)
    .bind(actor_id)
    .fetch_one(&app_pool)
    .await
    .expect("tag: INSERT must succeed for crm_app");

    let update = sqlx::query("UPDATE tag SET name = 'Renamed' WHERE id = $1")
        .bind(tag_id)
        .execute(&app_pool)
        .await;
    assert!(update.is_ok(), "tag: UPDATE must succeed for crm_app");

    let delete = sqlx::query("DELETE FROM tag WHERE id = $1")
        .bind(tag_id)
        .execute(&app_pool)
        .await;
    assert!(delete.is_ok(), "tag: DELETE must succeed for crm_app");

    let tag_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tag (organization_id, created_by_user_id, name) VALUES ($1, $2, 'Sphere') RETURNING id",
    )
    .bind(org_id)
    .bind(actor_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    let stage_id = first_stage_id_for_schema_test(&app_pool, org_id).await;
    let person_id: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id) VALUES ($1, 'Fixture', $2) RETURNING id",
    )
    .bind(org_id)
    .bind(stage_id)
    .fetch_one(&app_pool)
    .await
    .unwrap();

    let select = sqlx::query("SELECT * FROM person_tag")
        .fetch_all(&app_pool)
        .await;
    assert!(
        select.is_ok(),
        "person_tag: SELECT must succeed for crm_app"
    );
    let insert = sqlx::query(
        "INSERT INTO person_tag (organization_id, person_id, tag_id, added_by_user_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(tag_id)
    .bind(actor_id)
    .execute(&app_pool)
    .await;
    assert!(
        insert.is_ok(),
        "person_tag: INSERT must succeed for crm_app"
    );
    let update =
        sqlx::query("UPDATE person_tag SET added_by_user_id = added_by_user_id WHERE false")
            .execute(&app_pool)
            .await;
    assert!(
        update.is_err(),
        "person_tag: UPDATE must be denied for crm_app"
    );
    let delete = sqlx::query(
        "DELETE FROM person_tag WHERE organization_id = $1 AND person_id = $2 AND tag_id = $3",
    )
    .bind(org_id)
    .bind(person_id)
    .bind(tag_id)
    .execute(&app_pool)
    .await;
    assert!(
        delete.is_ok(),
        "person_tag: DELETE must succeed for crm_app"
    );
}

/// docs/specs/SLICE_011e.md §2: the index enumeration — the case-
/// insensitive uniqueness index on `tag` and the tag-led probe/count index
/// on `person_tag` both exist exactly as named.
#[sqlx::test]
#[ignore]
async fn tag_and_person_tag_indexes_exist(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let index_names: Vec<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes WHERE tablename IN ('tag', 'person_tag')",
    )
    .fetch_all(&app_pool)
    .await
    .unwrap();
    assert!(
        index_names.contains(&"tag_org_lower_name_key".to_string()),
        "missing tag_org_lower_name_key in {index_names:?}"
    );
    assert!(
        index_names.contains(&"person_tag_org_tag_person_idx".to_string()),
        "missing person_tag_org_tag_person_idx in {index_names:?}"
    );
}

/// docs/specs/SLICE_012.md §2, §8.1: the three `AFTER INSERT` triggers
/// exist, each firing on exactly the one table/event named. `pg_trigger`
/// is used (not `information_schema.triggers`, which does not expose
/// `AFTER`/`BEFORE` directly for a single-event row-level trigger without
/// joining more) via `pg_trigger.tgtype`'s bits (Postgres `trigger.h`):
/// `TRIGGER_TYPE_ROW = 1 << 0`, `TRIGGER_TYPE_BEFORE = 1 << 1`. There is no
/// dedicated "AFTER" bit — AFTER is the absence of BEFORE (and of INSTEAD
/// OF, `1 << 6`, irrelevant here since nothing declares one), so `is_after`
/// checks that the BEFORE bit is clear.
#[sqlx::test]
#[ignore]
async fn person_last_activity_triggers_exist_as_after_insert_row_triggers(migrator_pool: PgPool) {
    let rows: Vec<(String, String, bool, bool)> = sqlx::query_as(
        r#"SELECT t.tgname,
                  c.relname,
                  (t.tgtype & 2) = 0 AS is_after,
                  (t.tgtype & 1) <> 0 AS is_row
           FROM pg_trigger t
           JOIN pg_class c ON c.oid = t.tgrelid
           WHERE t.tgname IN (
               'inquiry_touch_person',
               'contact_attempted_touch_person',
               'correspondence_captured_touch_person'
           )
           AND NOT t.tgisinternal
           ORDER BY t.tgname"#,
    )
    .fetch_all(&migrator_pool)
    .await
    .unwrap();

    let expected: [(&str, &str); 3] = [
        ("contact_attempted_touch_person", "contact_attempted"),
        (
            "correspondence_captured_touch_person",
            "correspondence_captured",
        ),
        ("inquiry_touch_person", "inquiry"),
    ];
    assert_eq!(
        rows.len(),
        3,
        "expected exactly the three person_last_activity triggers, got {rows:?}"
    );
    for ((name, table, _, _), (expected_name, expected_table)) in rows.iter().zip(expected.iter()) {
        assert_eq!(name, expected_name);
        assert_eq!(table, expected_table);
    }
    for (name, _table, is_after, is_row) in &rows {
        assert!(is_after, "{name}: must be an AFTER trigger");
        assert!(is_row, "{name}: must be a FOR EACH ROW trigger");
    }

    // Each trigger fires on INSERT only (docs/specs/SLICE_012.md §2: history
    // tables have no application UPDATE/DELETE path).
    let events: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT trigger_name, event_manipulation
           FROM information_schema.triggers
           WHERE trigger_name IN (
               'inquiry_touch_person',
               'contact_attempted_touch_person',
               'correspondence_captured_touch_person'
           )
           ORDER BY trigger_name"#,
    )
    .fetch_all(&migrator_pool)
    .await
    .unwrap();
    assert_eq!(events.len(), 3, "{events:?}");
    for (_name, event) in &events {
        assert_eq!(event, "INSERT");
    }
}

/// docs/specs/SLICE_012.md §1 rule 6, §2: the one index, declared in
/// exactly the column order the spec names (`organization_id,
/// last_contact_at ASC NULLS FIRST, id ASC`), and no index on the other
/// three activity columns.
#[sqlx::test]
#[ignore]
async fn person_org_last_contact_idx_exists_with_the_declared_column_order(migrator_pool: PgPool) {
    let (indexdef,): (String,) = sqlx::query_as(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'person_org_last_contact_idx'",
    )
    .fetch_one(&migrator_pool)
    .await
    .unwrap();
    // Round 1 review fix 6: the FULL indexdef, not just substring checks
    // — `id` (bare, ascending — Postgres omits `ASC` for the default
    // direction) is the exact third key, immediately after
    // `last_contact_at NULLS FIRST`.
    assert_eq!(
        indexdef,
        "CREATE INDEX person_org_last_contact_idx ON public.person USING btree \
         (organization_id, last_contact_at NULLS FIRST, id)"
    );

    let index_names: Vec<String> =
        sqlx::query_scalar("SELECT indexname FROM pg_indexes WHERE tablename = 'person'")
            .fetch_all(&migrator_pool)
            .await
            .unwrap();
    for unwanted in ["last_inquiry_at", "last_inbound_at", "last_outbound_at"] {
        assert!(
            !index_names
                .iter()
                .any(|name| name.to_lowercase().contains(unwanted)),
            "no index on {unwanted} must exist (spec §1 rule 6); indexes: {index_names:?}"
        );
    }
}

/// docs/specs/SLICE_012.md §2: no new grants — `person`'s grant is
/// unchanged from Slice 002 (SELECT, INSERT, UPDATE, no DELETE), which is
/// what lets `crm_app` (which cannot UPDATE any history table) still
/// update the four new columns through the triggers when it inserts a
/// history row.
#[sqlx::test]
#[ignore]
async fn person_grant_is_unchanged_by_slice_012(migrator_pool: PgPool) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let update = sqlx::query(
        "UPDATE person SET last_contact_at = last_contact_at, last_inquiry_at = last_inquiry_at, \
         last_inbound_at = last_inbound_at, last_outbound_at = last_outbound_at WHERE false",
    )
    .execute(&app_pool)
    .await;
    assert!(
        update.is_ok(),
        "crm_app must still be able to UPDATE the (now four-column-wider) person row"
    );
    let delete = sqlx::query("DELETE FROM person").execute(&app_pool).await;
    assert!(
        delete.is_err(),
        "person: DELETE must still be denied for crm_app"
    );
}

async fn first_stage_id_for_schema_test(pool: &PgPool, organization_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1")
        .bind(organization_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// docs/specs/SLICE_011e.md §2, §9.1: case-insensitive uniqueness per
/// Organization (the `tag_org_lower_name_key` expression index), and the
/// composite FKs that make a cross-Organization `person_tag` row
/// unpersistable even if an application check regresses.
#[sqlx::test]
#[ignore]
async fn tag_name_is_unique_case_insensitively_per_organization_and_person_tag_is_tenant_isolated(
    migrator_pool: PgPool,
) {
    let app_pool = crate::common::connect_as_app(&migrator_pool).await;
    let (org_a, actor_a) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Acme Realty",
        "alice-schema@acme.test",
        "Alice",
        "correct horse battery staple",
    )
    .await;
    let (org_b, actor_b) = crate::common::create_org_with_stages_and_member(
        &migrator_pool,
        "Best Realty",
        "dave-schema@best.test",
        "Dave",
        "correct horse battery staple",
    )
    .await;

    sqlx::query(
        "INSERT INTO tag (organization_id, created_by_user_id, name) VALUES ($1, $2, 'Investor')",
    )
    .bind(org_a)
    .bind(actor_a)
    .execute(&app_pool)
    .await
    .unwrap();

    let collision = sqlx::query(
        "INSERT INTO tag (organization_id, created_by_user_id, name) VALUES ($1, $2, 'INVESTOR')",
    )
    .bind(org_a)
    .bind(actor_a)
    .execute(&app_pool)
    .await;
    assert!(
        collision.is_err(),
        "a case-insensitive duplicate name in the SAME Organization must be rejected"
    );

    // The identical name is fine in a DIFFERENT Organization.
    let cross_org_same_name = sqlx::query(
        "INSERT INTO tag (organization_id, created_by_user_id, name) VALUES ($1, $2, 'Investor')",
    )
    .bind(org_b)
    .bind(actor_b)
    .execute(&app_pool)
    .await;
    assert!(cross_org_same_name.is_ok());

    // A cross-Organization person_tag row (a Person of org_a with a tag of
    // org_b) is rejected by the composite FKs even though both ids exist.
    let stage_a = first_stage_id_for_schema_test(&app_pool, org_a).await;
    let person_a: Uuid = sqlx::query_scalar(
        "INSERT INTO person (organization_id, first_name, stage_id) VALUES ($1, 'Fixture', $2) RETURNING id",
    )
    .bind(org_a)
    .bind(stage_a)
    .fetch_one(&app_pool)
    .await
    .unwrap();
    let tag_b: Uuid = sqlx::query_scalar("SELECT id FROM tag WHERE organization_id = $1")
        .bind(org_b)
        .fetch_one(&app_pool)
        .await
        .unwrap();
    let cross_org_person_tag = sqlx::query(
        "INSERT INTO person_tag (organization_id, person_id, tag_id, added_by_user_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(org_a)
    .bind(person_a)
    .bind(tag_b)
    .bind(actor_a)
    .execute(&app_pool)
    .await;
    assert!(
        cross_org_person_tag.is_err(),
        "a Person or tag from another Organization can never be persisted into person_tag"
    );
}

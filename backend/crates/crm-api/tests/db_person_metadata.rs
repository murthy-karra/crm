//! Direct typed-command compatibility for Mobile006; no HTTP adapter authority.
use crm_api::{
    domain::{
        custom_field::{self, CreateCustomField, CustomFieldValue, FieldType},
        envelope::{CommandContext, Origin},
        note::{self, AddNote},
        person_metadata::{
            self, MetadataAction, MetadataChange, MetadataError, UpdatePersonMetadata,
        },
        tag::{self, AddPersonTag, CreateTag, RemovePersonTag},
    },
    ids::{CorrelationId, OrganizationId, PersonId, TagId, UserId},
    realtime::Publisher,
};
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    pool: PgPool,
    ctx: CommandContext,
    person: PersonId,
}
async fn fixture(migrator: &PgPool) -> Fixture {
    let (org, actor) = crate::common::create_org_with_stages_and_member(
        migrator,
        "Metadata typed fixture",
        "metadata-typed@synthetic.test",
        "Synthetic actor",
        "synthetic-metadata-password",
    )
    .await;
    sqlx::query(
        "UPDATE organization_membership SET role='admin' WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(org)
    .bind(actor)
    .execute(migrator)
    .await
    .unwrap();
    let pool = crate::common::connect_as_app(migrator).await;
    let person: Uuid = sqlx::query_scalar("INSERT INTO person(organization_id,first_name,stage_id) SELECT $1,'Metadata fixture',id FROM stage WHERE organization_id=$1 ORDER BY position LIMIT 1 RETURNING id")
        .bind(org).fetch_one(&pool).await.unwrap();
    Fixture {
        pool,
        person: PersonId::new(person),
        ctx: CommandContext {
            organization_id: OrganizationId::new(org),
            actor_user_id: UserId::new(actor),
            origin: Origin::MobileSession,
            correlation_id: CorrelationId::new(Uuid::new_v4()),
        },
    }
}
async fn tokens(f: &Fixture) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT metadata_revision,c.revision,mobile_revision,details_revision FROM person p JOIN mobile_metadata_catalog c ON c.organization_id=p.organization_id WHERE p.organization_id=$1 AND p.id=$2")
        .bind(f.ctx.organization_id.0).bind(f.person.0).fetch_one(&f.pool).await.unwrap()
}
async fn tag(f: &Fixture, label: &str) -> TagId {
    tag::create_tag(&f.pool, &f.ctx, CreateTag { name: label.into() })
        .await
        .unwrap()
        .tag
        .id
}
async fn apply(
    f: &Fixture,
    expected: (i64, i64),
    actions: Vec<MetadataAction>,
) -> Result<MetadataChange, MetadataError> {
    let mut tx = f.pool.begin().await.unwrap();
    let result = person_metadata::update_person_metadata_in_transaction(
        &mut tx,
        &f.ctx,
        UpdatePersonMetadata {
            person_id: f.person,
            expected_metadata_revision: expected.0,
            expected_catalog_revision: expected.1,
            actions,
        },
    )
    .await?;
    tx.commit().await.unwrap();
    Ok(result)
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, typed metadata atomicity"]
async fn metadata_typed_final_tag_capacity_and_all_field_types_are_atomic(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let publisher = Publisher::Disabled;
    let mut tags = Vec::new();
    for n in 0..21 {
        let id = tag(&f, &format!("Typed tag {n}")).await;
        if n < 20 {
            tag::add_person_tag(
                &f.pool,
                &publisher,
                &f.ctx,
                AddPersonTag {
                    person_id: f.person,
                    tag_id: id,
                },
            )
            .await
            .unwrap();
        }
        tags.push(id);
    }
    let mut fields = Vec::new();
    for (label, field_type) in [
        ("Text", FieldType::Text),
        ("Number", FieldType::Number),
        ("Date", FieldType::Date),
        ("Choice", FieldType::Choice),
    ] {
        fields.push(
            custom_field::create_custom_field(
                &f.pool,
                &f.ctx,
                CreateCustomField {
                    label: label.into(),
                    field_type,
                    options: if field_type == FieldType::Choice {
                        vec!["North".into(), "South".into()]
                    } else {
                        vec![]
                    },
                },
            )
            .await
            .unwrap()
            .field,
        );
    }
    let before = tokens(&f).await;
    let result = apply(
        &f,
        (before.0, before.1),
        vec![
            MetadataAction::AddTag { tag_id: tags[20] },
            MetadataAction::SetField {
                field_id: fields[0].id,
                value: CustomFieldValue::Text("  Retained text  ".into()),
            },
            MetadataAction::RemoveTag { tag_id: tags[0] },
            MetadataAction::SetField {
                field_id: fields[1].id,
                value: CustomFieldValue::Number("123456.1250".into()),
            },
            MetadataAction::SetField {
                field_id: fields[2].id,
                value: CustomFieldValue::Date(
                    chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
                ),
            },
            MetadataAction::SetField {
                field_id: fields[3].id,
                value: CustomFieldValue::Choice(fields[3].options[0].id),
            },
        ],
    )
    .await
    .unwrap();
    assert!(result.tags && result.fields);
    let after = tokens(&f).await;
    assert_eq!(after, (before.0 + 6, before.1, before.2 + 6, before.3));
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM person_tag WHERE person_id=$1),(SELECT count(*) FROM person_custom_field_value WHERE person_id=$1)").bind(f.person.0).fetch_one(&f.pool).await.unwrap();
    assert_eq!(counts, (20, 4));
    let number:String=sqlx::query_scalar("SELECT number_value::text FROM person_custom_field_value WHERE person_id=$1 AND field_id=$2").bind(f.person.0).bind(fields[1].id.0).fetch_one(&f.pool).await.unwrap();
    assert_eq!(number, "123456.1250");

    // Intentionally commit after the returned validation error: atomicity must
    // come from preparing the complete patch, not merely dropping a transaction.
    let mut tx = f.pool.begin().await.unwrap();
    let result = person_metadata::update_person_metadata_in_transaction(
        &mut tx,
        &f.ctx,
        UpdatePersonMetadata {
            person_id: f.person,
            expected_metadata_revision: after.0,
            expected_catalog_revision: after.1,
            actions: vec![
                MetadataAction::RemoveTag { tag_id: tags[1] },
                MetadataAction::SetField {
                    field_id: fields[1].id,
                    value: CustomFieldValue::Number("1e4".into()),
                },
            ],
        },
    )
    .await;
    assert!(matches!(
        result,
        Err(MetadataError::Field(
            custom_field::CustomFieldError::InvalidValue
        ))
    ));
    tx.commit().await.unwrap();
    assert_eq!(tokens(&f).await, after);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM person_tag WHERE person_id=$1 AND tag_id=$2)"
    )
    .bind(f.person.0)
    .bind(tags[1].0)
    .fetch_one(&f.pool)
    .await
    .unwrap());
    let unchanged = apply(
        &f,
        (after.0, after.1),
        vec![MetadataAction::SetField {
            field_id: fields[1].id,
            value: CustomFieldValue::Number("123456.125".into()),
        }],
    )
    .await
    .unwrap();
    assert!(!unchanged.tags && !unchanged.fields);
    assert_eq!(tokens(&f).await, after);
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, tag foreign-key cascade revision coverage"]
async fn metadata_tag_foreign_key_cascade_advances_without_catalog_fanout(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let tag_id = tag(&f, "FK cascade").await;
    let other_person:Uuid=sqlx::query_scalar("INSERT INTO person(organization_id,first_name,stage_id) SELECT $1,'Unaffected Person',stage_id FROM person WHERE id=$2 RETURNING id")
        .bind(f.ctx.organization_id.0).bind(f.person.0).fetch_one(&f.pool).await.unwrap();
    let before = tokens(&f).await;
    apply(
        &f,
        (before.0, before.1),
        vec![MetadataAction::AddTag { tag_id }],
    )
    .await
    .unwrap();
    let linked = tokens(&f).await;
    // The typed delete explicitly removes links; this separate fixture proves
    // the FK path itself remains covered by the all-writer derived trigger.
    sqlx::query("DELETE FROM tag WHERE organization_id=$1 AND id=$2")
        .bind(f.ctx.organization_id.0)
        .bind(tag_id.0)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        tokens(&f).await,
        (linked.0 + 1, linked.1 + 1, linked.2 + 1, linked.3)
    );
    let unaffected: (i64, i64) =
        sqlx::query_as("SELECT metadata_revision,mobile_revision FROM person WHERE id=$1")
            .bind(other_person)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(
        unaffected,
        (1, 1),
        "catalog deletion does not fan out to unlinked People"
    );
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, typed metadata conflict scope"]
async fn metadata_typed_conflict_detects_aba_but_allows_unrelated_note(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let publisher = Publisher::Disabled;
    let tag_id = tag(&f, "ABA").await;
    let before = tokens(&f).await;
    note::add_note(
        &f.pool,
        &publisher,
        &f.ctx,
        AddNote {
            person_id: f.person,
            body: "Unrelated synthetic note".into(),
        },
    )
    .await
    .unwrap();
    let with_note = tokens(&f).await;
    assert_eq!(
        (with_note.0, with_note.1, with_note.3),
        (before.0, before.1, before.3)
    );
    assert!(with_note.2 > before.2);
    let unchanged = apply(
        &f,
        (before.0, before.1),
        vec![MetadataAction::RemoveTag { tag_id }],
    )
    .await
    .unwrap();
    assert!(!unchanged.tags && !unchanged.fields);
    tag::add_person_tag(
        &f.pool,
        &publisher,
        &f.ctx,
        AddPersonTag {
            person_id: f.person,
            tag_id,
        },
    )
    .await
    .unwrap();
    tag::remove_person_tag(
        &f.pool,
        &publisher,
        &f.ctx,
        RemovePersonTag {
            person_id: f.person,
            tag_id,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        apply(
            &f,
            (before.0, before.1),
            vec![MetadataAction::AddTag { tag_id }]
        )
        .await,
        Err(MetadataError::RevisionConflict)
    ));
    assert_eq!(tokens(&f).await.0, before.0 + 2);
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, direct metadata authority"]
async fn metadata_typed_rechecks_actor_and_tenant_before_noop(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let tag_id = tag(&f, "Authority").await;
    let before = tokens(&f).await;
    let (foreign_org, foreign_actor) = crate::common::create_org_with_stages_and_member(
        &migrator,
        "Other metadata Org",
        "foreign-metadata@synthetic.test",
        "Foreign actor",
        "synthetic-metadata-password",
    )
    .await;
    for (org, actor) in [
        (f.ctx.organization_id.0, foreign_actor),
        (foreign_org, foreign_actor),
    ] {
        let mut tx = f.pool.begin().await.unwrap();
        let ctx = CommandContext {
            organization_id: OrganizationId::new(org),
            actor_user_id: UserId::new(actor),
            origin: Origin::MobileSession,
            correlation_id: CorrelationId::new(Uuid::new_v4()),
        };
        let result = person_metadata::update_person_metadata_in_transaction(
            &mut tx,
            &ctx,
            UpdatePersonMetadata {
                person_id: f.person,
                expected_metadata_revision: before.0,
                expected_catalog_revision: before.1,
                actions: vec![MetadataAction::RemoveTag { tag_id }],
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(MetadataError::Forbidden | MetadataError::NotFound)
        ));
        tx.rollback().await.unwrap();
    }
    sqlx::query("UPDATE organization_membership SET status='inactive' WHERE organization_id=$1 AND user_id=$2").bind(f.ctx.organization_id.0).bind(f.ctx.actor_user_id.0).execute(&migrator).await.unwrap();
    assert!(matches!(
        apply(
            &f,
            (before.0, before.1),
            vec![MetadataAction::RemoveTag { tag_id }]
        )
        .await,
        Err(MetadataError::Forbidden)
    ));
    assert_eq!(tokens(&f).await, before);
    sqlx::query("UPDATE organization_membership SET status='active' WHERE organization_id=$1 AND user_id=$2")
        .bind(f.ctx.organization_id.0).bind(f.ctx.actor_user_id.0).execute(&migrator).await.unwrap();
    // Explicit negative fixture: even a no-op by the current admin must use
    // the operational gate, independently of any HTTP/session adapter.
    sqlx::query("UPDATE organization SET workspace_mode='migration_review',workspace_revision=workspace_revision+1 WHERE id=$1")
        .bind(f.ctx.organization_id.0).execute(&migrator).await.unwrap();
    let result = apply(
        &f,
        (before.0, before.1),
        vec![MetadataAction::RemoveTag { tag_id }],
    )
    .await;
    assert!(
        matches!(result,Err(MetadataError::Database(sqlx::Error::Database(ref error))) if error.code().as_deref()==Some("P010C"))
    );
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, archived metadata and option isolation"]
async fn metadata_typed_archived_clear_and_foreign_option_preserve_atomicity(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let tag_id = tag(&f, "Atomic sibling").await;
    let field = custom_field::create_custom_field(
        &f.pool,
        &f.ctx,
        CreateCustomField {
            label: "Choice to archive".into(),
            field_type: FieldType::Choice,
            options: vec!["Native option".into()],
        },
    )
    .await
    .unwrap()
    .field;
    let (foreign_org, foreign_actor) = crate::common::create_org_with_stages_and_member(
        &migrator,
        "Foreign option Org",
        "foreign-option@synthetic.test",
        "Foreign option actor",
        "synthetic-metadata-password",
    )
    .await;
    let foreign_field: Uuid = sqlx::query_scalar("INSERT INTO custom_field(organization_id,label,field_type,position,created_by_user_id) VALUES($1,'Foreign choice','choice',0,$2) RETURNING id")
        .bind(foreign_org).bind(foreign_actor).fetch_one(&f.pool).await.unwrap();
    let foreign_option: Uuid = sqlx::query_scalar("INSERT INTO custom_field_option(organization_id,field_id,label,position) VALUES($1,$2,'Foreign option',0) RETURNING id")
        .bind(foreign_org).bind(foreign_field).fetch_one(&f.pool).await.unwrap();
    let before = tokens(&f).await;
    let invalid = apply(
        &f,
        (before.0, before.1),
        vec![
            MetadataAction::AddTag { tag_id },
            MetadataAction::SetField {
                field_id: field.id,
                value: CustomFieldValue::Choice(crm_api::ids::CustomFieldOptionId::new(
                    foreign_option,
                )),
            },
        ],
    )
    .await;
    assert!(matches!(
        invalid,
        Err(MetadataError::Field(
            custom_field::CustomFieldError::UnknownOption
        ))
    ));
    assert_eq!(tokens(&f).await, before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_tag WHERE person_id=$1")
            .bind(f.person.0)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
    apply(
        &f,
        (before.0, before.1),
        vec![MetadataAction::SetField {
            field_id: field.id,
            value: CustomFieldValue::Choice(field.options[0].id),
        }],
    )
    .await
    .unwrap();
    let populated = tokens(&f).await;
    custom_field::update_custom_field(
        &f.pool,
        &f.ctx,
        custom_field::UpdateCustomField {
            field_id: field.id,
            label: field.label.clone(),
            archived: true,
        },
    )
    .await
    .unwrap();
    let archived = tokens(&f).await;
    assert_eq!(
        archived,
        (populated.0, populated.1 + 1, populated.2, populated.3)
    );
    assert!(matches!(
        apply(
            &f,
            (populated.0, populated.1),
            vec![MetadataAction::ClearField { field_id: field.id }]
        )
        .await,
        Err(MetadataError::CatalogRevisionConflict)
    ));
    assert!(matches!(
        apply(
            &f,
            (archived.0, archived.1),
            vec![MetadataAction::SetField {
                field_id: field.id,
                value: CustomFieldValue::Choice(field.options[0].id)
            }]
        )
        .await,
        Err(MetadataError::Field(
            custom_field::CustomFieldError::FieldArchived
        ))
    ));
    assert!(
        apply(
            &f,
            (archived.0, archived.1),
            vec![MetadataAction::ClearField { field_id: field.id }]
        )
        .await
        .unwrap()
        .fields
    );
    let cleared = tokens(&f).await;
    assert_eq!(
        cleared,
        (archived.0 + 1, archived.1, archived.2 + 1, archived.3)
    );
    assert!(
        !apply(
            &f,
            (cleared.0, cleared.1),
            vec![MetadataAction::ClearField { field_id: field.id }]
        )
        .await
        .unwrap()
        .fields
    );
    assert_eq!(tokens(&f).await, cleared);
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, metadata overflow and erasure"]
async fn metadata_derived_tokens_overflow_rolls_back_but_person_erasure_succeeds(migrator: PgPool) {
    let f = fixture(&migrator).await;
    let linked = tag(&f, "Already linked").await;
    let added = tag(&f, "Would overflow").await;
    let before = tokens(&f).await;
    apply(
        &f,
        (before.0, before.1),
        vec![MetadataAction::AddTag { tag_id: linked }],
    )
    .await
    .unwrap();
    for expression in ["metadata_revision+1", "metadata_revision+2", "0"] {
        let error = sqlx::query(&format!(
            "UPDATE person SET metadata_revision={expression} WHERE id=$1"
        ))
        .bind(f.person.0)
        .execute(&f.pool)
        .await
        .expect_err("app cannot forge derived token");
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("22003")
        );
    }
    let error = sqlx::query(
        "UPDATE mobile_metadata_catalog SET revision=revision+1 WHERE organization_id=$1",
    )
    .bind(f.ctx.organization_id.0)
    .execute(&f.pool)
    .await
    .expect_err("catalog token remains read-only");
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    // Privileged, isolated boundary fixture; ordinary callers cannot set this token.
    let mut setup = migrator.begin().await.unwrap();
    sqlx::query("ALTER TABLE person DISABLE TRIGGER mobile_metadata_person_revision")
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("UPDATE person SET metadata_revision=9223372036854775807 WHERE id=$1")
        .bind(f.person.0)
        .execute(&mut *setup)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE person ENABLE TRIGGER mobile_metadata_person_revision")
        .execute(&mut *setup)
        .await
        .unwrap();
    setup.commit().await.unwrap();
    let full = tokens(&f).await;
    for action in [
        MetadataAction::AddTag { tag_id: added },
        MetadataAction::RemoveTag { tag_id: linked },
    ] {
        let result = apply(&f, (full.0, full.1), vec![action]).await;
        let error = match result {
            Err(MetadataError::Tag(tag::TagError::Database(error))) => error,
            Err(MetadataError::Database(error)) => error,
            _ => panic!("metadata overflow must reject"),
        };
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("22003")
        );
        assert_eq!(tokens(&f).await, full);
    }
    sqlx::query(
        "UPDATE mobile_metadata_catalog SET revision=9223372036854775807 WHERE organization_id=$1",
    )
    .bind(f.ctx.organization_id.0)
    .execute(&migrator)
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tag WHERE organization_id=$1")
        .bind(f.ctx.organization_id.0)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert!(
        matches!(tag::create_tag(&f.pool,&f.ctx,CreateTag{name:"Catalog overflow".into()}).await,
        Err(tag::TagError::Database(ref error)) if error.as_database_error().unwrap().code().as_deref()==Some("22003"))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tag WHERE organization_id=$1")
            .bind(f.ctx.organization_id.0)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        count
    );
    sqlx::query("DELETE FROM person WHERE organization_id=$1 AND id=$2")
        .bind(f.ctx.organization_id.0)
        .bind(f.person.0)
        .execute(&migrator)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_tag WHERE person_id=$1")
            .bind(f.person.0)
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "isolated PostgreSQL, bounded catalog versus Person lock graph"]
async fn metadata_tag_delete_waits_for_shared_command_then_advances_both_tokens(migrator: PgPool) {
    use std::time::Duration;
    let f = fixture(&migrator).await;
    let tag_id = tag(&f, "Concurrent deletion").await;
    let before = tokens(&f).await;
    let mut tx = f.pool.begin().await.unwrap();
    person_metadata::update_person_metadata_in_transaction(
        &mut tx,
        &f.ctx,
        UpdatePersonMetadata {
            person_id: f.person,
            expected_metadata_revision: before.0,
            expected_catalog_revision: before.1,
            actions: vec![MetadataAction::AddTag { tag_id }],
        },
    )
    .await
    .unwrap();
    let pool = f.pool.clone();
    let ctx = CommandContext {
        organization_id: f.ctx.organization_id,
        actor_user_id: f.ctx.actor_user_id,
        origin: Origin::WebSession,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    let deletion =
        tokio::spawn(async move { tag::delete_tag(&pool, &ctx, tag::DeleteTag { tag_id }).await });
    tokio::time::timeout(Duration::from_secs(2),async {
        loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))")
                .fetch_one(&migrator).await.unwrap();
            if blocked { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("catalog deletion reaches the advisory barrier before a Person lock");
    tx.commit().await.unwrap();
    let deleted = tokio::time::timeout(Duration::from_secs(2), deletion)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(deleted.deleted);
    assert_eq!(deleted.removed_from_people, 1);
    let after = tokens(&f).await;
    assert_eq!(after, (before.0 + 2, before.1 + 1, before.2 + 2, before.3));
    assert!(matches!(
        apply(
            &f,
            (after.0, after.1),
            vec![MetadataAction::RemoveTag { tag_id }]
        )
        .await,
        Err(MetadataError::Tag(tag::TagError::NotFound))
    ));
    assert_eq!(tokens(&f).await, after);
}

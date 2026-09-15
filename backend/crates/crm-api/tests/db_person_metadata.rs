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

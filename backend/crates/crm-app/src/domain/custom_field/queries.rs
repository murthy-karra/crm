use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::ids::{CustomFieldId, CustomFieldOptionId, OrganizationId, PersonId, UserId};

use super::error::CustomFieldError;
use super::model::{CustomField, CustomFieldOption, CustomFieldValue, FieldType, Value};

// --- command-internal row types --------------------------------------------

/// A full `custom_field` row for command-internal lookups (create, rename,
/// archive/restore, option writes). Kept separate from the public
/// [`CustomField`] response shape, mirroring `tag::queries::TagRowFull`.
/// Carries no `id`: every caller already holds the field id it looked
/// this row up by (the command's own `field_id`), so the row need not
/// repeat it.
pub(crate) struct CustomFieldRowFull {
    pub label: String,
    pub field_type: FieldType,
    pub archived_at: Option<DateTime<Utc>>,
}

fn decode_field_row(
    label: String,
    field_type: String,
    archived_at: Option<DateTime<Utc>>,
) -> Result<CustomFieldRowFull, CustomFieldError> {
    Ok(CustomFieldRowFull {
        label,
        field_type: FieldType::from_db_str(&field_type).ok_or(CustomFieldError::Corrupt)?,
        archived_at,
    })
}

/// `UpdateCustomField`/`AddCustomFieldOption`/`UpdateCustomFieldOption`'s
/// `FOR UPDATE` load (docs/specs/SLICE_019.md §3): any state (live or
/// archived) — option writes are permitted on an archived field (spec
/// §13).
pub(crate) async fn lock_field_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<Option<CustomFieldRowFull>, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT label, field_type, archived_at
           FROM custom_field
           WHERE id = $1 AND organization_id = $2
           FOR UPDATE"#,
        field_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|r| decode_field_row(r.label, r.field_type, r.archived_at))
        .transpose()
}

/// `SetPersonCustomFieldValue`'s `FOR SHARE` load (docs/specs/SLICE_019.md
/// §3): makes a concurrent definition write (which takes `FOR UPDATE`
/// under the `custom_fields:<org>` lock) wait behind an in-flight value
/// write, so the archived/type check just taken stays exact.
pub(crate) async fn lock_field_for_share(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<Option<CustomFieldRowFull>, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT label, field_type, archived_at
           FROM custom_field
           WHERE id = $1 AND organization_id = $2
           FOR SHARE"#,
        field_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|r| decode_field_row(r.label, r.field_type, r.archived_at))
        .transpose()
}

/// `ClearPersonCustomFieldValue`'s field lookup (docs/specs/SLICE_019.md
/// §3): "loads the field in any state for the 404" — existence only, no
/// row lock (the value row's own delete is the only write).
pub(crate) async fn field_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<bool, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM custom_field WHERE id = $1 AND organization_id = $2"#,
        field_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

/// A live field other than `exclude_id` with the same case-insensitive
/// label (`CreateCustomField`/`UpdateCustomField`'s collision check, spec
/// §3): "the live-label uniqueness check runs only when the row is, or
/// becomes, live" — the caller skips this call entirely when the
/// resulting state is archived.
pub(crate) async fn other_field_with_lower_label_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    label: &str,
    exclude_id: Option<CustomFieldId>,
) -> Result<bool, CustomFieldError> {
    let exclude = exclude_id.map(|id| id.0);
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM custom_field
           WHERE organization_id = $1 AND archived_at IS NULL AND lower(label) = lower($2)
             AND ($3::uuid IS NULL OR id <> $3)"#,
        organization_id.0,
        label,
        exclude,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

/// Live field count for the 50-per-Organization cap (D-050).
pub(crate) async fn count_live_fields(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<i64, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!" FROM custom_field
           WHERE organization_id = $1 AND archived_at IS NULL"#,
        organization_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.count)
}

/// `max(live) + 1` for a new or restored field's `position` (spec §3).
pub(crate) async fn max_live_field_position(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<i32, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT COALESCE(MAX(position), 0) as "max!" FROM custom_field
           WHERE organization_id = $1 AND archived_at IS NULL"#,
        organization_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.max)
}

pub(crate) async fn insert_field(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    created_by_user_id: UserId,
    label: &str,
    field_type: FieldType,
    position: i32,
) -> Result<CustomFieldId, CustomFieldError> {
    let row = sqlx::query!(
        r#"INSERT INTO custom_field (organization_id, label, field_type, position, created_by_user_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id"#,
        organization_id.0,
        label,
        field_type.as_str(),
        position,
        created_by_user_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(CustomFieldId::new(row.id))
}

/// Full-replace update (spec §3): `position` is `Some(..)` only on an
/// archived-to-live transition (un-archive appends to the live order);
/// `None` leaves the stored position untouched.
pub(crate) async fn update_field(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    label: &str,
    archived_at: Option<DateTime<Utc>>,
    position: Option<i32>,
) -> Result<(), CustomFieldError> {
    sqlx::query!(
        r#"UPDATE custom_field
           SET label = $3, archived_at = $4, position = COALESCE($5, position), updated_at = now()
           WHERE id = $1 AND organization_id = $2"#,
        field_id.0,
        organization_id.0,
        label,
        archived_at,
        position,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The full live order, ordered `position, id` (`ReorderCustomFields`'s
/// validation set, spec §3).
pub(crate) async fn live_field_ids(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Vec<CustomFieldId>, CustomFieldError> {
    let rows = sqlx::query!(
        r#"SELECT id FROM custom_field
           WHERE organization_id = $1 AND archived_at IS NULL
           ORDER BY position, id"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|r| CustomFieldId::new(r.id)).collect())
}

/// Rewrites every listed field's `position` to its 1-based index
/// (`ReorderCustomFields`, spec §3) in one statement — the caller has
/// already verified `ordered_ids` is exactly the live set with no
/// duplicates.
pub(crate) async fn reorder_field_positions(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    ordered_ids: &[CustomFieldId],
) -> Result<(), CustomFieldError> {
    let ids: Vec<Uuid> = ordered_ids.iter().map(|id| id.0).collect();
    sqlx::query!(
        r#"UPDATE custom_field cf
           SET position = u.ord::integer, updated_at = now()
           FROM unnest($2::uuid[]) WITH ORDINALITY AS u(id, ord)
           WHERE cf.id = u.id AND cf.organization_id = $1"#,
        organization_id.0,
        &ids,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// --- option command-internal helpers ----------------------------------------

/// Carries no `id`: every caller already holds the option id it looked
/// this row up by (the `CustomFieldRowFull` precedent above).
pub(crate) struct OptionRowFull {
    pub label: String,
    pub archived_at: Option<DateTime<Utc>>,
}

pub(crate) async fn lock_option_for_update(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    option_id: CustomFieldOptionId,
) -> Result<Option<OptionRowFull>, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT label, archived_at FROM custom_field_option
           WHERE id = $1 AND field_id = $2 AND organization_id = $3
           FOR UPDATE"#,
        option_id.0,
        field_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| OptionRowFull {
        label: r.label,
        archived_at: r.archived_at,
    }))
}

/// `SetPersonCustomFieldValue`'s option check (spec §3): live, and bound
/// to this exact field — an archived, foreign, or nonexistent option id
/// all take this same `false` path, so the caller's `UnknownOption` is
/// byte-identical for all three.
pub(crate) async fn option_is_live_for_field(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    option_id: CustomFieldOptionId,
) -> Result<bool, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM custom_field_option
           WHERE id = $1 AND field_id = $2 AND organization_id = $3 AND archived_at IS NULL"#,
        option_id.0,
        field_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

pub(crate) async fn other_option_with_lower_label_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    label: &str,
    exclude_id: Option<CustomFieldOptionId>,
) -> Result<bool, CustomFieldError> {
    let exclude = exclude_id.map(|id| id.0);
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM custom_field_option
           WHERE field_id = $1 AND organization_id = $2 AND archived_at IS NULL
             AND lower(label) = lower($3) AND ($4::uuid IS NULL OR id <> $4)"#,
        field_id.0,
        organization_id.0,
        label,
        exclude,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.is_some())
}

pub(crate) async fn count_live_options(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<i64, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!" FROM custom_field_option
           WHERE organization_id = $1 AND field_id = $2 AND archived_at IS NULL"#,
        organization_id.0,
        field_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.count)
}

pub(crate) async fn max_live_option_position(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<i32, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT COALESCE(MAX(position), 0) as "max!" FROM custom_field_option
           WHERE organization_id = $1 AND field_id = $2 AND archived_at IS NULL"#,
        organization_id.0,
        field_id.0,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.max)
}

pub(crate) async fn insert_option(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    label: &str,
    position: i32,
) -> Result<CustomFieldOptionId, CustomFieldError> {
    let row = sqlx::query!(
        r#"INSERT INTO custom_field_option (organization_id, field_id, label, position)
           VALUES ($1, $2, $3, $4)
           RETURNING id"#,
        organization_id.0,
        field_id.0,
        label,
        position,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(CustomFieldOptionId::new(row.id))
}

pub(crate) async fn update_option(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    option_id: CustomFieldOptionId,
    label: &str,
    archived_at: Option<DateTime<Utc>>,
) -> Result<(), CustomFieldError> {
    sqlx::query!(
        r#"UPDATE custom_field_option
           SET label = $4, archived_at = $5, updated_at = now()
           WHERE id = $1 AND field_id = $2 AND organization_id = $3"#,
        option_id.0,
        field_id.0,
        organization_id.0,
        label,
        archived_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// --- reads --------------------------------------------------------------

/// Lightweight filter-reference lookup. Unlike `list_definitions`, this is
/// bounded by one ID and deliberately does not join/count Person values.
pub async fn live_field_type_for_filter(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<Option<FieldType>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT field_type FROM custom_field
           WHERE id = $1 AND organization_id = $2 AND archived_at IS NULL"#,
    )
    .bind(field_id.0)
    .bind(organization_id.0)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|row| {
        let field_type: String = row.try_get("field_type")?;
        FieldType::from_db_str(&field_type).ok_or_else(|| {
            sqlx::Error::Decode("custom field has an unrecognized field_type".into())
        })
    })
    .transpose()
}

/// Filter-option reference lookup. Archived options intentionally remain
/// valid: People may retain a value after an admin archives that option.
pub async fn option_ids_belong_to_field_for_filter(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
    option_ids: &[CustomFieldOptionId],
) -> Result<bool, sqlx::Error> {
    let ids: Vec<Uuid> = option_ids.iter().map(|id| id.0).collect();
    let row = sqlx::query(
        r#"SELECT count(*) as count FROM custom_field_option
           WHERE organization_id = $1 AND field_id = $2 AND id = ANY($3::uuid[])"#,
    )
    .bind(organization_id.0)
    .bind(field_id.0)
    .bind(&ids)
    .fetch_one(&mut *conn)
    .await?;
    let count: i64 = row.try_get("count")?;
    Ok(count == i64::try_from(ids.len()).unwrap_or(i64::MAX))
}

/// ID-bounded labels for filter descriptions. This intentionally avoids the
/// definition-list read model and its Person-value count aggregation.
pub async fn filter_names_for_fields(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_ids: &[CustomFieldId],
) -> Result<
    (
        HashMap<CustomFieldId, String>,
        HashMap<(CustomFieldId, CustomFieldOptionId), String>,
    ),
    sqlx::Error,
> {
    let ids: Vec<Uuid> = field_ids.iter().map(|id| id.0).collect();
    if ids.is_empty() {
        return Ok((HashMap::new(), HashMap::new()));
    }
    let fields = sqlx::query(
        "SELECT id, label FROM custom_field WHERE organization_id = $1 AND id = ANY($2::uuid[])",
    )
    .bind(organization_id.0)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let options = sqlx::query(
        "SELECT field_id, id, label FROM custom_field_option WHERE organization_id = $1 AND field_id = ANY($2::uuid[])",
    )
    .bind(organization_id.0)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let field_names = fields
        .into_iter()
        .map(|row| {
            Ok((
                CustomFieldId::new(row.try_get("id")?),
                row.try_get("label")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    let option_names = options
        .into_iter()
        .map(|row| {
            Ok((
                (
                    CustomFieldId::new(row.try_get("field_id")?),
                    CustomFieldOptionId::new(row.try_get("id")?),
                ),
                row.try_get("label")?,
            ))
        })
        .collect::<Result<HashMap<_, _>, sqlx::Error>>()?;
    Ok((field_names, option_names))
}

/// One field's full response shape, any state (docs/specs/SLICE_019.md
/// §3, §4) — used by every definition/option command's outcome so the
/// caller never has to re-derive `CustomField` from raw rows itself.
pub async fn load_custom_field(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<Option<CustomField>, CustomFieldError> {
    let field = sqlx::query!(
        r#"SELECT cf.id, cf.label, cf.field_type, cf.position, cf.archived_at,
                  count(v.person_id) as "person_count!"
           FROM custom_field cf
           LEFT JOIN person_custom_field_value v
             ON v.field_id = cf.id AND v.organization_id = cf.organization_id
           WHERE cf.id = $1 AND cf.organization_id = $2
           GROUP BY cf.id, cf.label, cf.field_type, cf.position, cf.archived_at"#,
        field_id.0,
        organization_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(field) = field else {
        return Ok(None);
    };
    let field_type = FieldType::from_db_str(&field.field_type).ok_or(CustomFieldError::Corrupt)?;

    let option_rows = sqlx::query!(
        r#"SELECT id, label, position, archived_at FROM custom_field_option
           WHERE organization_id = $1 AND field_id = $2
           ORDER BY position, id"#,
        organization_id.0,
        field_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;
    let options = option_rows
        .into_iter()
        .map(|o| CustomFieldOption {
            id: CustomFieldOptionId::new(o.id),
            label: o.label,
            position: o.position,
            archived_at: o.archived_at,
        })
        .collect();

    Ok(Some(CustomField {
        id: CustomFieldId::new(field.id),
        label: field.label,
        field_type,
        position: field.position,
        archived_at: field.archived_at,
        person_count: field.person_count,
        options,
    }))
}

/// `GET /api/custom-fields` (docs/specs/SLICE_019.md §4): live fields
/// first ordered `position, id`, then archived ordered `archived_at DESC,
/// id`. Two queries rather than one CASE-driven ORDER BY, kept simple and
/// auditable; the 50-live/effectively-unbounded-archived cap keeps this
/// unpaginated (the `tag::list_for_organization` precedent).
/// One field-aggregate row shared by both the live and archived halves of
/// [`list_definitions`] below. The two `sqlx::query!` calls each expand to
/// their own anonymous `Record` type even though the projected columns are
/// identical, so every row is normalized into this named struct
/// immediately — the two lists can then be `chain()`ed as the same
/// concrete type.
struct FieldAggregateRow {
    id: Uuid,
    label: String,
    field_type: String,
    position: i32,
    archived_at: Option<DateTime<Utc>>,
    person_count: i64,
}

pub async fn list_definitions(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Vec<CustomField>, CustomFieldError> {
    let live: Vec<FieldAggregateRow> = sqlx::query!(
        r#"SELECT cf.id, cf.label, cf.field_type, cf.position, cf.archived_at,
                  count(v.person_id) as "person_count!"
           FROM custom_field cf
           LEFT JOIN person_custom_field_value v
             ON v.field_id = cf.id AND v.organization_id = cf.organization_id
           WHERE cf.organization_id = $1 AND cf.archived_at IS NULL
           GROUP BY cf.id, cf.label, cf.field_type, cf.position, cf.archived_at
           ORDER BY cf.position, cf.id"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| FieldAggregateRow {
        id: r.id,
        label: r.label,
        field_type: r.field_type,
        position: r.position,
        archived_at: r.archived_at,
        person_count: r.person_count,
    })
    .collect();
    let archived: Vec<FieldAggregateRow> = sqlx::query!(
        r#"SELECT cf.id, cf.label, cf.field_type, cf.position, cf.archived_at,
                  count(v.person_id) as "person_count!"
           FROM custom_field cf
           LEFT JOIN person_custom_field_value v
             ON v.field_id = cf.id AND v.organization_id = cf.organization_id
           WHERE cf.organization_id = $1 AND cf.archived_at IS NOT NULL
           GROUP BY cf.id, cf.label, cf.field_type, cf.position, cf.archived_at
           ORDER BY cf.archived_at DESC, cf.id"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| FieldAggregateRow {
        id: r.id,
        label: r.label,
        field_type: r.field_type,
        position: r.position,
        archived_at: r.archived_at,
        person_count: r.person_count,
    })
    .collect();

    let option_rows = sqlx::query!(
        r#"SELECT id, field_id, label, position, archived_at FROM custom_field_option
           WHERE organization_id = $1
           ORDER BY field_id, position, id"#,
        organization_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut options_by_field: HashMap<Uuid, Vec<CustomFieldOption>> = HashMap::new();
    for o in option_rows {
        options_by_field
            .entry(o.field_id)
            .or_default()
            .push(CustomFieldOption {
                id: CustomFieldOptionId::new(o.id),
                label: o.label,
                position: o.position,
                archived_at: o.archived_at,
            });
    }

    let mut out = Vec::with_capacity(live.len() + archived.len());
    for row in live.into_iter().chain(archived) {
        let field_type =
            FieldType::from_db_str(&row.field_type).ok_or(CustomFieldError::Corrupt)?;
        out.push(CustomField {
            id: CustomFieldId::new(row.id),
            label: row.label,
            field_type,
            position: row.position,
            archived_at: row.archived_at,
            person_count: row.person_count,
            options: options_by_field.remove(&row.id).unwrap_or_default(),
        });
    }
    Ok(out)
}

/// A Person's set values on **live** fields only, in field position order
/// (docs/specs/SLICE_019.md §3, §4) — the `GET /api/people/{id}`
/// assembly and the Operator's `custom_fields` view share this one read.
/// The option label is carried even when the held option is archived
/// (spec §12 BOUNDARY case): the `LEFT JOIN` has no `archived_at`
/// filter.
pub async fn values_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<Value>, CustomFieldError> {
    let rows = sqlx::query!(
        r#"SELECT v.field_id, cf.label as field_label, cf.field_type,
                  v.text_value,
                  trim_scale(v.number_value)::text as "number_text?",
                  v.date_value,
                  v.option_id,
                  o.label as "option_label?",
                  v.updated_at
           FROM person_custom_field_value v
           JOIN custom_field cf
             ON cf.id = v.field_id AND cf.organization_id = v.organization_id
           LEFT JOIN custom_field_option o ON o.id = v.option_id
           WHERE v.organization_id = $1 AND v.person_id = $2 AND cf.archived_at IS NULL
           ORDER BY cf.position, cf.id"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let field_type =
            FieldType::from_db_str(&row.field_type).ok_or(CustomFieldError::Corrupt)?;
        let value = match field_type {
            FieldType::Text => {
                CustomFieldValue::Text(row.text_value.ok_or(CustomFieldError::Corrupt)?)
            }
            FieldType::Number => {
                CustomFieldValue::Number(row.number_text.ok_or(CustomFieldError::Corrupt)?)
            }
            FieldType::Date => {
                CustomFieldValue::Date(row.date_value.ok_or(CustomFieldError::Corrupt)?)
            }
            FieldType::Choice => CustomFieldValue::Choice(CustomFieldOptionId::new(
                row.option_id.ok_or(CustomFieldError::Corrupt)?,
            )),
        };
        out.push(Value {
            field_id: CustomFieldId::new(row.field_id),
            label: row.field_label,
            field_type,
            value,
            option_label: row.option_label,
            updated_at: row.updated_at,
        });
    }
    Ok(out)
}

/// The upsert behind `SetPersonCustomFieldValue` (docs/specs/SLICE_019.md
/// §3): target-state idempotent — `ON CONFLICT ... WHERE ... IS DISTINCT
/// FROM ...` skips the write (and the `updated_at`/`updated_by_user_id`
/// bump) entirely when the stored value already matches. The number
/// column is bound as `CAST($n::text AS numeric)` (spec §2: no decimal
/// crate in the workspace) — `number_text` is `None` unless `field_type`
/// is `number`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn upsert_value(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    field_id: CustomFieldId,
    field_type: FieldType,
    text_value: Option<&str>,
    number_text: Option<&str>,
    date_value: Option<NaiveDate>,
    option_id: Option<Uuid>,
    actor_user_id: UserId,
    origin: &str,
    correlation_id: Uuid,
) -> Result<bool, CustomFieldError> {
    let result = sqlx::query!(
        r#"INSERT INTO person_custom_field_value
             (organization_id, person_id, field_id, field_type, text_value, number_value,
              date_value, option_id, updated_by_user_id, origin, correlation_id)
           VALUES ($1, $2, $3, $4, $5, CAST($6::text AS numeric), $7, $8, $9, $10, $11)
           ON CONFLICT (organization_id, person_id, field_id) DO UPDATE
             SET text_value = EXCLUDED.text_value,
                 number_value = EXCLUDED.number_value,
                 date_value = EXCLUDED.date_value,
                 option_id = EXCLUDED.option_id,
                 updated_by_user_id = EXCLUDED.updated_by_user_id,
                 origin = EXCLUDED.origin,
                 correlation_id = EXCLUDED.correlation_id,
                 updated_at = now()
           WHERE person_custom_field_value.text_value IS DISTINCT FROM EXCLUDED.text_value
              OR person_custom_field_value.number_value IS DISTINCT FROM EXCLUDED.number_value
              OR person_custom_field_value.date_value IS DISTINCT FROM EXCLUDED.date_value
              OR person_custom_field_value.option_id IS DISTINCT FROM EXCLUDED.option_id"#,
        organization_id.0,
        person_id.0,
        field_id.0,
        field_type.as_str(),
        text_value,
        number_text,
        date_value,
        option_id,
        actor_user_id.0,
        origin,
        correlation_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn delete_value(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
    field_id: CustomFieldId,
) -> Result<bool, CustomFieldError> {
    let result = sqlx::query!(
        r#"DELETE FROM person_custom_field_value
           WHERE organization_id = $1 AND person_id = $2 AND field_id = $3"#,
        organization_id.0,
        person_id.0,
        field_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

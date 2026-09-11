use std::collections::HashSet;

use chrono::Utc;
use sqlx::{PgConnection, PgPool};

use crate::domain::admin::Role;
use crate::domain::envelope::CommandContext;
use crate::domain::person::queries as person_queries;
use crate::ids::{CustomFieldId, CustomFieldOptionId, OrganizationId, PersonId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

use super::error::CustomFieldError;
use super::model::{
    validate_date_range, validate_number_pattern, validate_text_value, CustomField,
    CustomFieldValue, FieldType, Value,
};
use super::queries::{self, CustomFieldRowFull};

const MAX_LIVE_FIELDS: i64 = 50;
const MAX_LIVE_OPTIONS: i64 = 50;

/// The field/option label shape (docs/specs/SLICE_019.md §2 CHECK, the
/// `tag::normalize_and_validate_name` pattern with a 60-character cap):
/// trimmed, 1–60 code points, no control character anywhere (narrower
/// than the CHECK's ASCII-only `btrim`, safe in the write direction — the
/// `TaskTitle` precedent's stated reasoning). Review round 1, B8: U+2028
/// (LINE SEPARATOR) and U+2029 (PARAGRAPH SEPARATOR) are also rejected —
/// they are line breaks like `\n`/`\r` but are not `is_control()` in Rust
/// (Unicode category Zl/Zp, not Cc), while the DB CHECK's POSIX
/// `[[:cntrl:]]` class (glibc) does treat them as control, so leaving them
/// out here would accept a label the database then refuses — the exact
/// `TaskTitle::parse` precedent (LATER batch 2026-09-10, item 2).
pub fn normalize_and_validate_label(raw: &str) -> Result<String, CustomFieldError> {
    let label = raw.trim();
    if label.is_empty()
        || label.chars().count() > 60
        || label
            .chars()
            .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}')
    {
        return Err(CustomFieldError::MalformedRequest);
    }
    Ok(label.to_owned())
}

/// `CreateCustomField`'s `options` validation (docs/specs/SLICE_019.md
/// §3): non-choice types must supply none (`TypeMismatch`); a choice type
/// must supply 1–50, each a valid label, distinct case-insensitively
/// (`InvalidValue` otherwise — content problems, not the field's own
/// label, use this code per the command table in spec §4).
fn validate_new_field_options(
    field_type: FieldType,
    raw_options: &[String],
) -> Result<Vec<String>, CustomFieldError> {
    if field_type != FieldType::Choice {
        if !raw_options.is_empty() {
            return Err(CustomFieldError::TypeMismatch);
        }
        return Ok(Vec::new());
    }
    if raw_options.is_empty() || raw_options.len() > MAX_LIVE_OPTIONS as usize {
        return Err(CustomFieldError::InvalidValue);
    }
    let mut seen_lower = HashSet::with_capacity(raw_options.len());
    let mut normalized = Vec::with_capacity(raw_options.len());
    for raw in raw_options {
        let label =
            normalize_and_validate_label(raw).map_err(|_| CustomFieldError::InvalidValue)?;
        if !seen_lower.insert(label.to_lowercase()) {
            return Err(CustomFieldError::InvalidValue);
        }
        normalized.push(label);
    }
    Ok(normalized)
}

/// The seven commands' shared advisory lock (docs/specs/SLICE_019.md §3):
/// one namespace for every definition/option write in an Organization, the
/// `tag::acquire_tags_lock` pattern — makes `CreateCustomField`'s
/// label-uniqueness-then-insert atomic with the 50-field quota check, and
/// gives every definition/option writer a single lock order so two admins
/// cannot race a duplicate label or a position collision into existence.
async fn acquire_custom_fields_lock(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), CustomFieldError> {
    let organization_id_text = organization_id.to_string();
    sqlx::query!(
        r#"SELECT pg_advisory_xact_lock(hashtextextended('custom_fields:' || $1::text, 0))"#,
        organization_id_text,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Every definition/option command's own-membership re-check (docs/specs/
/// SLICE_019.md §3, §9): a `FOR SHARE` re-read of the actor's OWN
/// membership row, inside the same transaction as the `custom_fields:<org>`
/// lock, so a concurrent demotion or deactivation cannot interleave with
/// the admin-only permission decision. `None` for a missing or inactive
/// membership — the actor already holds a valid authenticated session
/// (`AuthContext`/`OrgAdminContext`), so this is a concurrent-demotion
/// defense, not an authentication check; the caller folds `None` into the
/// same `Forbidden` every other failed verdict produces (the
/// `tag::lock_current_membership`/`task::lock_current_membership`
/// precedent — duplicated here rather than shared, the brief's stated
/// lane choice).
async fn lock_current_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    actor_user_id: UserId,
) -> Result<Option<Role>, CustomFieldError> {
    let row = sqlx::query!(
        r#"SELECT role, status
           FROM organization_membership
           WHERE organization_id = $1 AND user_id = $2
           FOR SHARE"#,
        organization_id.0,
        actor_user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.status != "active" {
        return Ok(None);
    }
    Ok(Some(
        Role::from_db_str(&row.role).ok_or(CustomFieldError::Corrupt)?,
    ))
}

/// Definitions and options are Organization-admin only (D-058 §2, spec
/// §3) — unlike tags, there is no "creator while unused" carve-out: a
/// custom-field definition is Organization schema, not a member's own
/// object.
fn require_admin(role: Option<Role>) -> Result<(), CustomFieldError> {
    match role {
        Some(Role::Admin) => Ok(()),
        _ => Err(CustomFieldError::Forbidden),
    }
}

fn record_outcome<T>(result: &Result<T, CustomFieldError>) {
    if let Err(error) = result {
        tracing::warn!(error_kind = error.kind(), "custom field command failed");
        tracing::Span::current().record("outcome", error.kind());
    }
}

async fn publish_custom_field_changed(
    publisher: &Publisher,
    ctx: &CommandContext,
    person_id: PersonId,
) {
    let event = RealtimeEvent::person_changed(
        ctx.organization_id,
        Utc::now(),
        ctx.correlation_id,
        person_id,
        PersonChange::CustomFieldChanged,
    );
    publisher
        .publish_after_commit(Publication::for_event(event))
        .await;
}

async fn load_custom_field_or_corrupt(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<CustomField, CustomFieldError> {
    queries::load_custom_field(conn, organization_id, field_id)
        .await?
        // The row was just written inside this same transaction — its
        // absence here would mean the database lost a committed insert.
        .ok_or(CustomFieldError::Corrupt)
}

// --- CreateCustomField -------------------------------------------------------

/// No `Debug` derive: this struct carries an admin-authored label (and,
/// for a choice field, option labels), so `?cmd` can never put one in a
/// span or log (docs/specs/SLICE_019.md §9).
pub struct CreateCustomField {
    pub label: String,
    pub field_type: FieldType,
    pub options: Vec<String>,
}

/// `Debug` derives cleanly: `CustomField`'s own hand-written, redacting
/// impl is what actually runs for the `field` field (the
/// `TaskWithPerson`/`Task` precedent), so this composes safely without a
/// second hand-written impl.
#[derive(Debug, Clone)]
pub struct CreateCustomFieldOutcome {
    pub field: CustomField,
}

#[tracing::instrument(
    name = "custom_field.create",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        field_id = tracing::field::Empty,
        field_type = cmd.field_type.as_str(),
        outcome = tracing::field::Empty,
    )
)]
pub async fn create_custom_field(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: CreateCustomField,
) -> Result<CreateCustomFieldOutcome, CustomFieldError> {
    let result = create_custom_field_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            let span = tracing::Span::current();
            span.record("field_id", outcome.field.id.to_string());
            span.record("outcome", "created");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn create_custom_field_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: CreateCustomField,
) -> Result<CreateCustomFieldOutcome, CustomFieldError> {
    let label = normalize_and_validate_label(&cmd.label)?;
    let options = validate_new_field_options(cmd.field_type, &cmd.options)?;

    let mut tx = pool.begin().await?;
    acquire_custom_fields_lock(&mut tx, ctx.organization_id).await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    require_admin(role)?;

    if queries::other_field_with_lower_label_exists(&mut tx, ctx.organization_id, &label, None)
        .await?
    {
        return Err(CustomFieldError::LabelTaken);
    }
    let live_count = queries::count_live_fields(&mut tx, ctx.organization_id).await?;
    if live_count >= MAX_LIVE_FIELDS {
        return Err(CustomFieldError::LimitReached);
    }
    let position = queries::max_live_field_position(&mut tx, ctx.organization_id).await? + 1;

    let field_id = queries::insert_field(
        &mut tx,
        ctx.organization_id,
        ctx.actor_user_id,
        &label,
        cmd.field_type,
        position,
    )
    .await?;

    for (i, option_label) in options.iter().enumerate() {
        queries::insert_option(
            &mut tx,
            ctx.organization_id,
            field_id,
            option_label,
            i as i32 + 1,
        )
        .await?;
    }

    let field = load_custom_field_or_corrupt(&mut tx, ctx.organization_id, field_id).await?;
    tx.commit().await?;
    Ok(CreateCustomFieldOutcome { field })
}

// --- UpdateCustomField --------------------------------------------------------

pub struct UpdateCustomField {
    pub field_id: CustomFieldId,
    pub label: String,
    pub archived: bool,
}

/// `Debug` derives cleanly: `CustomField`'s own redacting impl runs for
/// `field` (the `CreateCustomFieldOutcome` precedent above).
#[derive(Debug, Clone)]
pub struct UpdateCustomFieldOutcome {
    pub field: CustomField,
    pub changed: bool,
}

#[tracing::instrument(
    name = "custom_field.update",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        field_id = %cmd.field_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn update_custom_field(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateCustomField,
) -> Result<UpdateCustomFieldOutcome, CustomFieldError> {
    let result = update_custom_field_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn update_custom_field_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateCustomField,
) -> Result<UpdateCustomFieldOutcome, CustomFieldError> {
    let label = normalize_and_validate_label(&cmd.label)?;

    let mut tx = pool.begin().await?;
    acquire_custom_fields_lock(&mut tx, ctx.organization_id).await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    require_admin(role)?;

    let row = queries::lock_field_for_update(&mut tx, ctx.organization_id, cmd.field_id)
        .await?
        .ok_or(CustomFieldError::NotFound)?;

    let currently_live = row.archived_at.is_none();
    if row.label == label && currently_live == !cmd.archived {
        let field =
            load_custom_field_or_corrupt(&mut tx, ctx.organization_id, cmd.field_id).await?;
        tx.commit().await?;
        return Ok(UpdateCustomFieldOutcome {
            field,
            changed: false,
        });
    }

    // The live-label uniqueness check runs only when the row is, or
    // becomes, live (spec §3).
    if !cmd.archived
        && queries::other_field_with_lower_label_exists(
            &mut tx,
            ctx.organization_id,
            &label,
            Some(cmd.field_id),
        )
        .await?
    {
        return Err(CustomFieldError::LabelTaken);
    }

    // Review round 1, B2: renaming an ALREADY-archived row (a different
    // label, so the no-op check above did not short-circuit) must not
    // re-stamp `archived_at` to now — keep the row's own original
    // timestamp when it already has one, and only mint a fresh one on a
    // genuine live-to-archived transition.
    let new_archived_at = if cmd.archived {
        row.archived_at.or_else(|| Some(Utc::now()))
    } else {
        None
    };
    // Un-archiving appends to the live order; archiving and a plain
    // rename leave the stored position untouched.
    let new_position = if !currently_live && !cmd.archived {
        Some(queries::max_live_field_position(&mut tx, ctx.organization_id).await? + 1)
    } else {
        None
    };

    queries::update_field(
        &mut tx,
        ctx.organization_id,
        cmd.field_id,
        &label,
        new_archived_at,
        new_position,
    )
    .await?;

    let field = load_custom_field_or_corrupt(&mut tx, ctx.organization_id, cmd.field_id).await?;
    tx.commit().await?;
    Ok(UpdateCustomFieldOutcome {
        field,
        changed: true,
    })
}

// --- ReorderCustomFields -------------------------------------------------------

pub struct ReorderCustomFields {
    pub field_ids: Vec<CustomFieldId>,
}

#[derive(Debug, Clone)]
pub struct ReorderCustomFieldsOutcome {
    pub fields: Vec<CustomField>,
}

#[tracing::instrument(
    name = "custom_field.reorder",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn reorder_custom_fields(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: ReorderCustomFields,
) -> Result<ReorderCustomFieldsOutcome, CustomFieldError> {
    let result = reorder_custom_fields_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "reordered");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn reorder_custom_fields_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: ReorderCustomFields,
) -> Result<ReorderCustomFieldsOutcome, CustomFieldError> {
    let mut tx = pool.begin().await?;
    acquire_custom_fields_lock(&mut tx, ctx.organization_id).await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    require_admin(role)?;

    let live_ids = queries::live_field_ids(&mut tx, ctx.organization_id).await?;
    let submitted: HashSet<CustomFieldId> = cmd.field_ids.iter().copied().collect();
    if submitted.len() != cmd.field_ids.len() || submitted.len() != live_ids.len() {
        return Err(CustomFieldError::InvalidValue);
    }
    let live_set: HashSet<CustomFieldId> = live_ids.into_iter().collect();
    if submitted != live_set {
        return Err(CustomFieldError::InvalidValue);
    }

    queries::reorder_field_positions(&mut tx, ctx.organization_id, &cmd.field_ids).await?;

    let fields = queries::list_definitions(&mut tx, ctx.organization_id).await?;
    tx.commit().await?;
    Ok(ReorderCustomFieldsOutcome { fields })
}

// --- AddCustomFieldOption ------------------------------------------------------

/// No `Debug` derive: carries an admin-authored option label
/// (docs/specs/SLICE_019.md §9).
pub struct AddCustomFieldOption {
    pub field_id: CustomFieldId,
    pub label: String,
}

/// `Debug` derives cleanly: `CustomField`'s own redacting impl runs for
/// `field` (the `CreateCustomFieldOutcome` precedent above).
#[derive(Debug, Clone)]
pub struct AddCustomFieldOptionOutcome {
    pub field: CustomField,
}

#[tracing::instrument(
    name = "custom_field_option.add",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        field_id = %cmd.field_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn add_custom_field_option(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: AddCustomFieldOption,
) -> Result<AddCustomFieldOptionOutcome, CustomFieldError> {
    let result = add_custom_field_option_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(_) => {
            tracing::Span::current().record("outcome", "added");
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn add_custom_field_option_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: AddCustomFieldOption,
) -> Result<AddCustomFieldOptionOutcome, CustomFieldError> {
    let label = normalize_and_validate_label(&cmd.label)?;

    let mut tx = pool.begin().await?;
    acquire_custom_fields_lock(&mut tx, ctx.organization_id).await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    require_admin(role)?;

    let field: CustomFieldRowFull =
        queries::lock_field_for_update(&mut tx, ctx.organization_id, cmd.field_id)
            .await?
            .ok_or(CustomFieldError::NotFound)?;
    if field.field_type != FieldType::Choice {
        return Err(CustomFieldError::TypeMismatch);
    }

    if queries::other_option_with_lower_label_exists(
        &mut tx,
        ctx.organization_id,
        cmd.field_id,
        &label,
        None,
    )
    .await?
    {
        return Err(CustomFieldError::OptionLabelTaken);
    }
    let live_count =
        queries::count_live_options(&mut tx, ctx.organization_id, cmd.field_id).await?;
    if live_count >= MAX_LIVE_OPTIONS {
        return Err(CustomFieldError::OptionLimitReached);
    }
    let position =
        queries::max_live_option_position(&mut tx, ctx.organization_id, cmd.field_id).await? + 1;
    queries::insert_option(&mut tx, ctx.organization_id, cmd.field_id, &label, position).await?;

    let field = load_custom_field_or_corrupt(&mut tx, ctx.organization_id, cmd.field_id).await?;
    tx.commit().await?;
    Ok(AddCustomFieldOptionOutcome { field })
}

// --- UpdateCustomFieldOption ---------------------------------------------------

pub struct UpdateCustomFieldOption {
    pub field_id: CustomFieldId,
    pub option_id: CustomFieldOptionId,
    pub label: String,
    pub archived: bool,
}

/// `Debug` derives cleanly: `CustomField`'s own redacting impl runs for
/// `field` (the `CreateCustomFieldOutcome` precedent above).
#[derive(Debug, Clone)]
pub struct UpdateCustomFieldOptionOutcome {
    pub field: CustomField,
    pub changed: bool,
}

#[tracing::instrument(
    name = "custom_field_option.update",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        field_id = %cmd.field_id,
        option_id = %cmd.option_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn update_custom_field_option(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateCustomFieldOption,
) -> Result<UpdateCustomFieldOptionOutcome, CustomFieldError> {
    let result = update_custom_field_option_attempt(pool, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn update_custom_field_option_attempt(
    pool: &PgPool,
    ctx: &CommandContext,
    cmd: UpdateCustomFieldOption,
) -> Result<UpdateCustomFieldOptionOutcome, CustomFieldError> {
    let label = normalize_and_validate_label(&cmd.label)?;

    let mut tx = pool.begin().await?;
    acquire_custom_fields_lock(&mut tx, ctx.organization_id).await?;
    let role = lock_current_membership(&mut tx, ctx.organization_id, ctx.actor_user_id).await?;
    require_admin(role)?;

    // The field itself must exist (any state — option writes are
    // permitted on an archived field, spec §13); a nonexistent field id
    // is 404, identical to a nonexistent option id.
    queries::lock_field_for_update(&mut tx, ctx.organization_id, cmd.field_id)
        .await?
        .ok_or(CustomFieldError::NotFound)?;
    let option =
        queries::lock_option_for_update(&mut tx, ctx.organization_id, cmd.field_id, cmd.option_id)
            .await?
            .ok_or(CustomFieldError::NotFound)?;

    let currently_live = option.archived_at.is_none();
    if option.label == label && currently_live == !cmd.archived {
        let field =
            load_custom_field_or_corrupt(&mut tx, ctx.organization_id, cmd.field_id).await?;
        tx.commit().await?;
        return Ok(UpdateCustomFieldOptionOutcome {
            field,
            changed: false,
        });
    }

    if !cmd.archived
        && queries::other_option_with_lower_label_exists(
            &mut tx,
            ctx.organization_id,
            cmd.field_id,
            &label,
            Some(cmd.option_id),
        )
        .await?
    {
        return Err(CustomFieldError::OptionLabelTaken);
    }

    // Review round 1, B2 (the field-rename precedent above): keep the
    // option's own original `archived_at` on a rename-while-archived,
    // never re-stamp it to now.
    let new_archived_at = if cmd.archived {
        option.archived_at.or_else(|| Some(Utc::now()))
    } else {
        None
    };
    queries::update_option(
        &mut tx,
        ctx.organization_id,
        cmd.field_id,
        cmd.option_id,
        &label,
        new_archived_at,
    )
    .await?;

    let field = load_custom_field_or_corrupt(&mut tx, ctx.organization_id, cmd.field_id).await?;
    tx.commit().await?;
    Ok(UpdateCustomFieldOptionOutcome {
        field,
        changed: true,
    })
}

// --- SetPersonCustomFieldValue -------------------------------------------------

/// No `Debug` derive: carries a Person's custom-field value (docs/specs/
/// SLICE_019.md §9 — "labels and values never appear in spans, logs,
/// error envelopes, the ledger or the realtime payload").
pub struct SetPersonCustomFieldValue {
    pub person_id: PersonId,
    pub field_id: CustomFieldId,
    pub value: CustomFieldValue,
}

/// `Debug` derives cleanly: `Value`'s own hand-written, redacting impl is
/// what actually runs for each element of `values` (the
/// `CreateCustomFieldOutcome` precedent above).
#[derive(Debug, Clone)]
pub struct PersonCustomFieldOutcome {
    pub values: Vec<Value>,
    pub changed: bool,
}

#[tracing::instrument(
    name = "person_custom_field.set",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        field_id = %cmd.field_id,
        value_chars = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn set_person_custom_field_value(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: SetPersonCustomFieldValue,
) -> Result<PersonCustomFieldOutcome, CustomFieldError> {
    if let CustomFieldValue::Text(text) = &cmd.value {
        tracing::Span::current().record("value_chars", text.chars().count());
    }
    let result = set_person_custom_field_value_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn set_person_custom_field_value_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: SetPersonCustomFieldValue,
) -> Result<PersonCustomFieldOutcome, CustomFieldError> {
    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(CustomFieldError::NotFound)?;
    let field = queries::lock_field_for_share(&mut tx, ctx.organization_id, cmd.field_id)
        .await?
        .ok_or(CustomFieldError::NotFound)?;
    if field.archived_at.is_some() {
        return Err(CustomFieldError::FieldArchived);
    }
    if field.field_type != cmd.value.field_type() {
        return Err(CustomFieldError::TypeMismatch);
    }

    // Pure-function validation AFTER the lock/archived/type-mismatch checks
    // above (spec §3, §4 precedence: NotFound, then FieldArchived, then
    // TypeMismatch, only then InvalidValue) — review round 1, B1. A
    // malformed value on the WRONG field or an archived field must report
    // that fact, not a content problem the field can't even hold.
    let (text_value, number_text, date_value) = match &cmd.value {
        CustomFieldValue::Text(raw) => (Some(validate_text_value(raw)?), None, None),
        CustomFieldValue::Number(raw) => {
            validate_number_pattern(raw)?;
            (None, Some(raw.clone()), None)
        }
        CustomFieldValue::Date(date) => {
            validate_date_range(*date)?;
            (None, None, Some(*date))
        }
        CustomFieldValue::Choice(_) => (None, None, None),
    };

    let option_id = match &cmd.value {
        CustomFieldValue::Choice(option_id) => {
            let live = queries::option_is_live_for_field(
                &mut tx,
                ctx.organization_id,
                cmd.field_id,
                *option_id,
            )
            .await?;
            if !live {
                return Err(CustomFieldError::UnknownOption);
            }
            Some(option_id.as_uuid())
        }
        _ => None,
    };

    let changed = queries::upsert_value(
        &mut tx,
        ctx.organization_id,
        cmd.person_id,
        cmd.field_id,
        field.field_type,
        text_value.as_deref(),
        number_text.as_deref(),
        date_value,
        option_id,
        ctx.actor_user_id,
        ctx.origin.as_str(),
        ctx.correlation_id.as_uuid(),
    )
    .await?;

    let values = queries::values_for_person(&mut tx, ctx.organization_id, cmd.person_id).await?;
    tx.commit().await?;

    if changed {
        publish_custom_field_changed(publisher, ctx, cmd.person_id).await;
    }
    Ok(PersonCustomFieldOutcome { values, changed })
}

// --- ClearPersonCustomFieldValue -----------------------------------------------

pub struct ClearPersonCustomFieldValue {
    pub person_id: PersonId,
    pub field_id: CustomFieldId,
}

#[tracing::instrument(
    name = "person_custom_field.clear",
    skip_all,
    fields(
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        person_id = %cmd.person_id,
        field_id = %cmd.field_id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn clear_person_custom_field_value(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ClearPersonCustomFieldValue,
) -> Result<PersonCustomFieldOutcome, CustomFieldError> {
    let result = clear_person_custom_field_value_attempt(pool, publisher, ctx, cmd).await;
    match &result {
        Ok(outcome) => {
            tracing::Span::current().record(
                "outcome",
                if outcome.changed {
                    "changed"
                } else {
                    "unchanged"
                },
            );
        }
        Err(_) => record_outcome(&result),
    }
    result
}

async fn clear_person_custom_field_value_attempt(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ClearPersonCustomFieldValue,
) -> Result<PersonCustomFieldOutcome, CustomFieldError> {
    let mut tx = pool.begin().await?;
    person_queries::lock_person(&mut tx, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(CustomFieldError::NotFound)?;
    // Any state (spec §3): permitted on an archived field.
    if !queries::field_exists(&mut tx, ctx.organization_id, cmd.field_id).await? {
        return Err(CustomFieldError::NotFound);
    }

    let changed =
        queries::delete_value(&mut tx, ctx.organization_id, cmd.person_id, cmd.field_id).await?;
    let values = queries::values_for_person(&mut tx, ctx.organization_id, cmd.person_id).await?;
    tx.commit().await?;

    if changed {
        publish_custom_field_changed(publisher, ctx, cmd.person_id).await;
    }
    Ok(PersonCustomFieldOutcome { values, changed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_validate_label_trims_and_bounds() {
        assert_eq!(
            normalize_and_validate_label("  Budget  ").unwrap(),
            "Budget"
        );
        assert!(normalize_and_validate_label("").is_err());
        assert!(normalize_and_validate_label("   ").is_err());
        assert!(normalize_and_validate_label(&"a".repeat(61)).is_err());
        assert!(normalize_and_validate_label(&"a".repeat(60)).is_ok());
        assert!(normalize_and_validate_label("bad\ttab").is_err());
        assert!(normalize_and_validate_label("bad\nline").is_err());
    }

    /// Review round 1, B8: U+2028/U+2029 are not `is_control()` in Rust but
    /// the DB CHECK's POSIX `[[:cntrl:]]` class (glibc) treats them as
    /// control — reject both here too (the `TaskTitle` precedent).
    #[test]
    fn normalize_and_validate_label_rejects_line_and_paragraph_separators() {
        assert!(normalize_and_validate_label("a\u{2028}b").is_err());
        assert!(normalize_and_validate_label("a\u{2029}b").is_err());
    }

    #[test]
    fn validate_new_field_options_rejects_options_on_non_choice_types() {
        assert!(matches!(
            validate_new_field_options(FieldType::Text, &["a".to_string()]),
            Err(CustomFieldError::TypeMismatch)
        ));
        assert!(validate_new_field_options(FieldType::Text, &[]).is_ok());
    }

    #[test]
    fn validate_new_field_options_requires_nonempty_bounded_distinct_choice_options() {
        assert!(matches!(
            validate_new_field_options(FieldType::Choice, &[]),
            Err(CustomFieldError::InvalidValue)
        ));
        let too_many: Vec<String> = (0..51).map(|i| format!("Option {i}")).collect();
        assert!(matches!(
            validate_new_field_options(FieldType::Choice, &too_many),
            Err(CustomFieldError::InvalidValue)
        ));
        assert!(matches!(
            validate_new_field_options(
                FieldType::Choice,
                &["Cold".to_string(), "cold".to_string()]
            ),
            Err(CustomFieldError::InvalidValue)
        ));
        let ok = validate_new_field_options(
            FieldType::Choice,
            &["Cold".to_string(), "Warm".to_string(), "Hot".to_string()],
        )
        .unwrap();
        assert_eq!(ok, vec!["Cold", "Warm", "Hot"]);
    }
}

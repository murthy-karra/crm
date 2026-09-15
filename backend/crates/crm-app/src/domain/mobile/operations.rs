use super::*;
use crate::domain::{
    commands::{
        self, change_person_stage_in_transaction, update_person_details_in_transaction,
        ChangePersonStage, ContactChannel, ContactDetailOperation, ContactOutcome,
        DetailContactKind, LogContactAttemptInTransaction, UpdatePersonDetails,
    },
    custom_field::CustomFieldValue,
    envelope::{CommandContext, Origin},
    note,
    person_metadata::{
        self, MetadataAction as TypedMetadataAction, MetadataError, UpdatePersonMetadata,
    },
    task,
};
use crate::ids::{ContactMethodId, CustomFieldId, PersonId, TagId, TaskId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub context_id: Uuid,
    pub operation_id: Uuid,
    pub kind: String,
    pub device_recorded_at: Option<DateTime<Utc>>,
    pub payload: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Add {
    person_id: Uuid,
    body: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Create {
    person_id: Uuid,
    title: String,
    kind: task::TaskKind,
    due_at: Option<DateTime<Utc>>,
    assignee_user_id: Option<Uuid>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    person_id: Uuid,
    target: Target,
}
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum Target {
    Downloaded(Downloaded),
    Created(Created),
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Downloaded {
    task_id: Uuid,
    expected_revision: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Created {
    created_by_operation_id: Uuid,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EditNote {
    person_id: Uuid,
    note_id: Uuid,
    expected_revision: String,
    body: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateTask {
    person_id: Uuid,
    task_id: Uuid,
    expected_revision: String,
    title: String,
    kind: task::TaskKind,
    due_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateTaskWire {
    person_id: Uuid,
    task_id: Uuid,
    expected_revision: String,
    title: String,
    kind: task::TaskKind,
    // `Value` keeps the field required under serde while retaining an explicit
    // JSON null. `Option<Option<T>>` cannot make that distinction: serde maps
    // both missing and null to the outer None.
    due_at: Value,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LogContactAttempt {
    person_id: Uuid,
    channel: ContactChannel,
    outcome: ContactOutcome,
    occurred_at: DateTime<Utc>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChangePersonStagePayload {
    person_id: Uuid,
    stage_id: Uuid,
    expected_stage_revision: String,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct UpdatePersonDetailsPayload {
    person_id: Uuid,
    expected_details_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_name: Option<Option<String>>,
    contact_operations: Vec<ContactOperationWire>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdatePersonMetadataPayload {
    person_id: Uuid,
    expected_metadata_revision: String,
    expected_catalog_revision: String,
    actions: Vec<MetadataAction>,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MetadataAction {
    AddTag {
        tag_id: Uuid,
    },
    RemoveTag {
        tag_id: Uuid,
    },
    SetField {
        field_id: Uuid,
        value: CustomFieldValue,
    },
    ClearField {
        field_id: Uuid,
    },
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum ContactOperationWire {
    Add { kind: String, value: String },
    Edit { id: Uuid, value: String },
    Remove { id: Uuid },
}
enum Payload {
    Add(Add),
    Create(Create),
    Complete(Complete),
    EditNote(EditNote),
    UpdateTask(UpdateTask),
    LogContactAttempt(LogContactAttempt),
    ChangePersonStage(ChangePersonStagePayload),
    UpdatePersonDetails(UpdatePersonDetailsPayload),
    UpdatePersonMetadata(UpdatePersonMetadataPayload),
}
impl Payload {
    fn parse(kind: &str, value: Value) -> Result<Self, MobileError> {
        Ok(match kind {
            "add_note" => {
                let mut v: Add = serde_json::from_value(value).map_err(|_| invalid())?;
                v.body = note::NoteBody::parse(&v.body)?;
                Self::Add(v)
            }
            "create_task" => {
                let mut v: Create = serde_json::from_value(value).map_err(|_| invalid())?;
                v.title = task::TaskTitle::parse(&v.title)?;
                Self::Create(v)
            }
            "complete_task" => {
                let v: Complete = serde_json::from_value(value).map_err(|_| invalid())?;
                if let Target::Downloaded(t) = &v.target {
                    revision(&t.expected_revision)?;
                }
                Self::Complete(v)
            }
            "edit_note" => {
                let mut v: EditNote = serde_json::from_value(value).map_err(|_| invalid())?;
                revision(&v.expected_revision)?;
                v.body = note::NoteBody::parse(&v.body)?;
                Self::EditNote(v)
            }
            "update_task" => {
                let wire: UpdateTaskWire = serde_json::from_value(value).map_err(|_| invalid())?;
                let mut v = UpdateTask {
                    person_id: wire.person_id,
                    task_id: wire.task_id,
                    expected_revision: wire.expected_revision,
                    title: wire.title,
                    kind: wire.kind,
                    due_at: if wire.due_at.is_null() {
                        None
                    } else {
                        Some(serde_json::from_value(wire.due_at).map_err(|_| invalid())?)
                    },
                };
                revision(&v.expected_revision)?;
                v.title = task::TaskTitle::parse(&v.title)?;
                Self::UpdateTask(v)
            }
            "log_contact_attempt" => {
                let mut v: LogContactAttempt =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                // PostgreSQL stores microseconds.  Normalizing before the
                // operation digest makes equivalent RFC3339 offsets and
                // sub-microsecond spellings converge on the exact fact time.
                v.occurred_at = normalize_contact_time(v.occurred_at)?;
                Self::LogContactAttempt(v)
            }
            "change_person_stage" => {
                let v: ChangePersonStagePayload =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                revision(&v.expected_stage_revision)?;
                Self::ChangePersonStage(v)
            }
            "update_person_details" => Self::UpdatePersonDetails(parse_details(value)?),
            "update_person_metadata" => Self::UpdatePersonMetadata(parse_metadata(value)?),
            _ => return Err(invalid()),
        })
    }
    fn person(&self) -> Uuid {
        match self {
            Self::Add(v) => v.person_id,
            Self::Create(v) => v.person_id,
            Self::Complete(v) => v.person_id,
            Self::EditNote(v) => v.person_id,
            Self::UpdateTask(v) => v.person_id,
            Self::LogContactAttempt(v) => v.person_id,
            Self::ChangePersonStage(v) => v.person_id,
            Self::UpdatePersonDetails(v) => v.person_id,
            Self::UpdatePersonMetadata(v) => v.person_id,
        }
    }
    fn json(&self) -> Result<Value, MobileError> {
        match self {
            Self::Add(v) => serialize(v),
            Self::Create(v) => serialize(v),
            Self::Complete(v) => serialize(v),
            Self::EditNote(v) => serialize(v),
            Self::UpdateTask(v) => serialize(v),
            Self::LogContactAttempt(v) => serialize(v),
            Self::ChangePersonStage(v) => serialize(v),
            Self::UpdatePersonDetails(v) => serialize(v),
            Self::UpdatePersonMetadata(v) => serialize(v),
        }
    }
}
fn parse_metadata(value: Value) -> Result<UpdatePersonMetadataPayload, MobileError> {
    let payload: UpdatePersonMetadataPayload =
        serde_json::from_value(value).map_err(|_| invalid())?;
    revision(&payload.expected_metadata_revision)?;
    revision(&payload.expected_catalog_revision)?;
    if payload.actions.is_empty() || payload.actions.len() > 50 {
        return Err(invalid());
    }
    let mut tags = std::collections::HashSet::new();
    let mut fields = std::collections::HashSet::new();
    for action in &payload.actions {
        let unique = match action {
            MetadataAction::AddTag { tag_id } | MetadataAction::RemoveTag { tag_id } => {
                tags.insert(*tag_id)
            }
            MetadataAction::SetField { field_id, .. } | MetadataAction::ClearField { field_id } => {
                fields.insert(*field_id)
            }
        };
        if !unique {
            return Err(invalid());
        }
    }
    Ok(payload)
}
fn parse_details(value: Value) -> Result<UpdatePersonDetailsPayload, MobileError> {
    let mut object = value.as_object().cloned().ok_or_else(invalid)?;
    let first_name = optional_name(object.remove("first_name"))?;
    let last_name = optional_name(object.remove("last_name"))?;
    let person_id = serde_json::from_value(object.remove("person_id").ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let expected_details_revision: String = serde_json::from_value(
        object
            .remove("expected_details_revision")
            .ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    revision(&expected_details_revision)?;
    let operations: Vec<ContactOperationWire> =
        serde_json::from_value(object.remove("contact_operations").ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    if !object.is_empty()
        || operations.is_empty() && first_name.is_none() && last_name.is_none()
        || operations.len() > 50
    {
        return Err(invalid());
    }
    let contact_operations = operations
        .into_iter()
        .map(|operation| match operation {
            ContactOperationWire::Add { kind, value } => Ok(ContactOperationWire::Add {
                kind,
                value: contact_value(value)?,
            }),
            ContactOperationWire::Edit { id, value } => Ok(ContactOperationWire::Edit {
                id,
                value: contact_value(value)?,
            }),
            ContactOperationWire::Remove { id } => Ok(ContactOperationWire::Remove { id }),
        })
        .collect::<Result<_, MobileError>>()?;
    Ok(UpdatePersonDetailsPayload {
        person_id,
        expected_details_revision,
        first_name,
        last_name,
        contact_operations,
    })
}
fn optional_name(value: Option<Value>) -> Result<Option<Option<String>>, MobileError> {
    value
        .map(|value| {
            if value.is_null() {
                Ok(None)
            } else {
                serde_json::from_value::<String>(value)
                    .map_err(|_| invalid())
                    .and_then(|value| clean_name(value).map(Some))
            }
        })
        .transpose()
}
fn clean_name(value: String) -> Result<String, MobileError> {
    let value = value.trim().to_owned();
    if value.chars().count() > 200 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(value)
}
fn contact_value(value: String) -> Result<String, MobileError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(value)
}
fn normalize_contact_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, MobileError> {
    use chrono::{Datelike, Timelike};

    // Keep the accepted domain inside the portable PostgreSQL/chrono range
    // used by this API.  A mobile contact has no artificial past-age limit.
    if !(1..=9999).contains(&value.year()) {
        return Err(invalid());
    }
    value
        .with_nanosecond((value.nanosecond() / 1_000) * 1_000)
        .ok_or_else(invalid)
}
fn revision(raw: &str) -> Result<i64, MobileError> {
    let n: i64 = raw.parse().map_err(|_| invalid())?;
    if n <= 0 || n.to_string() != raw {
        return Err(invalid());
    }
    Ok(n)
}
fn metadata_error(error: MetadataError) -> MobileError {
    match error {
        MetadataError::InvalidMetadata => code(422, "invalid_metadata"),
        MetadataError::NotFound => missing(),
        MetadataError::Forbidden => code(403, "forbidden"),
        MetadataError::RevisionConflict => code(409, "revision_conflict"),
        MetadataError::CatalogRevisionConflict => code(409, "catalog_revision_conflict"),
        MetadataError::Tag(crate::domain::tag::TagError::PersonTagLimitReached) => {
            code(422, "tag_limit_reached")
        }
        MetadataError::Tag(crate::domain::tag::TagError::NotFound)
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::NotFound)
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::FieldArchived)
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::TypeMismatch)
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::InvalidValue)
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::UnknownOption) => {
            code(422, "invalid_metadata")
        }
        MetadataError::Tag(crate::domain::tag::TagError::Forbidden)
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::Forbidden) => {
            code(403, "forbidden")
        }
        MetadataError::Database(error)
        | MetadataError::Tag(crate::domain::tag::TagError::Database(error))
        | MetadataError::Field(crate::domain::custom_field::CustomFieldError::Database(error)) => {
            error.into()
        }
        _ => code(503, "unavailable"),
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Receipt {
    pub operation_id: Uuid,
    pub outcome: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub committed_revision: Option<String>,
    pub person_revision: String,
    pub changed: bool,
    pub accepted_at: DateTime<Utc>,
    pub replayed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_contact_ids: Option<Vec<AddedContactId>>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddedContactId {
    pub ordinal: usize,
    pub id: Uuid,
}
struct Stored {
    receipt: Receipt,
    person: Uuid,
    context: Uuid,
    kind: String,
    key: String,
    digest: Vec<u8>,
}
fn stored(row: sqlx::postgres::PgRow) -> Stored {
    Stored {
        receipt: Receipt {
            operation_id: row.get("operation_id"),
            outcome: "accepted".into(),
            resource_type: row.get("resource_type"),
            resource_id: row.get("resource_id"),
            committed_revision: row
                .get::<Option<i64>, _>("committed_revision")
                .map(|r| r.to_string()),
            person_revision: row.get::<i64, _>("person_revision").to_string(),
            changed: row.get("changed"),
            accepted_at: row.get("accepted_at"),
            replayed: true,
            added_contact_ids: (row.get::<String, _>("kind") == "update_person_details").then(
                || {
                    row.get::<Option<Value>, _>("added_contact_ids")
                        .and_then(|value| serde_json::from_value(value).ok())
                        .unwrap_or_default()
                },
            ),
        },
        person: row.get("person_id"),
        context: row.get("context_id"),
        kind: row.get("kind"),
        key: row.get("digest_key_id"),
        digest: row.get("payload_digest"),
    }
}
async fn get(
    conn: &mut PgConnection,
    auth: &AuthContext,
    id: Uuid,
) -> Result<Option<Stored>, MobileError> {
    Ok(sqlx::query("SELECT * FROM mobile_operation_receipt WHERE organization_id=$1 AND actor_user_id=$2 AND operation_id=$3")
        .bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(id).fetch_optional(conn).await?.map(stored))
}
async fn visible(
    conn: &mut PgConnection,
    auth: &AuthContext,
    value: &Stored,
) -> Result<(), MobileError> {
    let sql=match value.receipt.resource_type.as_str(){
        "note"=>"SELECT EXISTS(SELECT 1 FROM note n JOIN person p ON p.id=n.person_id AND p.organization_id=n.organization_id WHERE n.organization_id=$1 AND n.person_id=$2 AND n.id=$3 AND n.deleted_at IS NULL)",
        "task"=>"SELECT EXISTS(SELECT 1 FROM task n JOIN person p ON p.id=n.person_id AND p.organization_id=n.organization_id WHERE n.organization_id=$1 AND n.person_id=$2 AND n.id=$3 AND n.deleted_at IS NULL)",
        "contact_attempt"=>"SELECT EXISTS(SELECT 1 FROM contact_attempted c JOIN person p ON p.id=c.person_id AND p.organization_id=c.organization_id WHERE c.organization_id=$1 AND c.person_id=$2 AND c.id=$3)",
        "person_stage"=>"SELECT EXISTS(SELECT 1 FROM person p WHERE p.organization_id=$1 AND p.id=$2 AND p.id=$3)",
        "person_details"=>"SELECT EXISTS(SELECT 1 FROM person p WHERE p.organization_id=$1 AND p.id=$2 AND p.id=$3)",
        "person_metadata"=>"SELECT EXISTS(SELECT 1 FROM person p WHERE p.organization_id=$1 AND p.id=$2 AND p.id=$3)",
        _=>return Err(code(503,"unavailable")),
    };
    if !sqlx::query_scalar::<_, bool>(sql)
        .bind(auth.active_organization_id.0)
        .bind(value.person)
        .bind(value.receipt.resource_id)
        .fetch_one(conn)
        .await?
    {
        return Err(missing());
    }
    Ok(())
}
#[tracing::instrument(name="mobile.operation",skip_all,fields(organization_id=%auth.active_organization_id,actor_id=%auth.actor_user_id,operation_id=%request.operation_id))]
pub async fn execute(
    pool: &PgPool,
    publisher: &Publisher,
    keys: &ReceiptKeys,
    auth: &AuthContext,
    header_context: Uuid,
    request: Operation,
) -> Result<Receipt, MobileError> {
    if header_context != request.context_id {
        return Err(code(401, "unauthenticated"));
    }
    let kind = request.kind.clone();
    let payload = Payload::parse(&kind, request.payload)?;
    let canonical=serde_json::to_vec(&json!({"version":1,"protocol":PROTOCOL,"context_id":request.context_id,"kind":request.kind,"device_recorded_at":request.device_recorded_at,"payload":payload.json()?})).map_err(|_|invalid())?;
    let mut tx = begin(pool, auth, false).await?;
    context(&mut tx, auth, request.context_id, true).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('crm-mobile-operation:'||$1::text||':'||$2::text||':'||$3::text,0))")
        .bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(request.operation_id).execute(&mut *tx).await?;
    // Replay checks current identity/workspace/resource, but performs no fresh
    // mutation authorization or baseline check on the already accepted action.
    if let Some(prior) = get(&mut tx, auth, request.operation_id).await? {
        authority(&mut tx, auth, true).await?;
        visible(&mut tx, auth, &prior).await?;
        if prior.context != request.context_id
            || !keys.matches(
                &prior.key,
                b"crm-mobile-operation-v1\0",
                &canonical,
                &prior.digest,
            )?
        {
            return Err(code(409, "operation_payload_mismatch"));
        }
        tx.commit().await?;
        return Ok(prior.receipt);
    }
    let person = PersonId(payload.person());
    if kind == "update_person_details" {
        let acquired: bool = sqlx::query_scalar(
            "SELECT pg_try_advisory_xact_lock(hashtextextended('intake:' || $1::text, 0))",
        )
        .bind(auth.active_organization_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        if !acquired {
            return Err(code(503, "intake_busy"));
        }
    }
    if kind != "update_person_metadata" {
        crate::domain::person::queries::lock_person(&mut tx, person, auth.active_organization_id)
            .await?
            .ok_or_else(missing)?;
        authority(&mut tx, auth, true).await?;
    }
    let mut ctx = CommandContext::from_auth(auth);
    ctx.origin = Origin::MobileSession;
    let mut metadata_change = None;
    let (resource_type, resource_id, changed, added_contact_ids) = match payload {
        Payload::Add(v) => {
            let n = note::add_note_in_transaction(
                &mut tx,
                &ctx,
                note::AddNote {
                    person_id: person,
                    body: v.body,
                },
            )
            .await?;
            ("note", n.id.0, true, Vec::new())
        }
        Payload::Create(v) => {
            let t = task::create_task_in_transaction(
                &mut tx,
                &ctx,
                task::CreateTask {
                    person_id: person,
                    title: v.title,
                    kind: v.kind,
                    due_at: v.due_at,
                    assignee_user_id: v.assignee_user_id.map(UserId),
                },
            )
            .await?;
            ("task", t.id.0, true, Vec::new())
        }
        Payload::Complete(v) => {
            let (id, expected) = match v.target {
                Target::Downloaded(t) => (t.task_id, revision(&t.expected_revision)?),
                Target::Created(c) => {
                    let prior = get(&mut tx, auth, c.created_by_operation_id)
                        .await?
                        .ok_or(code(409, "dependency_pending"))?;
                    if prior.kind != "create_task"
                        || prior.person != person.0
                        || prior.context != request.context_id
                    {
                        return Err(code(409, "dependency_pending"));
                    }
                    (
                        prior.receipt.resource_id,
                        revision(
                            prior
                                .receipt
                                .committed_revision
                                .as_deref()
                                .ok_or(code(503, "unavailable"))?,
                        )?,
                    )
                }
            };
            let result = task::complete_task_in_transaction(
                &mut tx,
                &ctx,
                task::CompleteTask {
                    person_id: person,
                    task_id: TaskId(id),
                },
                Some(expected),
            )
            .await?
            .ok_or(code(409, "revision_conflict"))?;
            ("task", id, result.changed, Vec::new())
        }
        Payload::EditNote(v) => {
            let result = note::edit_note_in_transaction(
                &mut tx,
                &ctx,
                note::EditNote {
                    person_id: person,
                    note_id: crate::ids::NoteId(v.note_id),
                    body: v.body,
                },
                Some(revision(&v.expected_revision)?),
            )
            .await?
            .ok_or(code(409, "revision_conflict"))?;
            ("note", v.note_id, result.changed, Vec::new())
        }
        Payload::UpdateTask(v) => {
            let result = task::update_task_in_transaction(
                &mut tx,
                &ctx,
                task::UpdateTask {
                    person_id: person,
                    task_id: TaskId(v.task_id),
                    title: v.title,
                    kind: v.kind,
                    due_at: v.due_at,
                    // This value is ignored by preserve_assignee mode; the
                    // current locked assignee, including NULL, is retained.
                    assignee_user_id: auth.actor_user_id,
                },
                Some(revision(&v.expected_revision)?),
                true,
            )
            .await?
            .ok_or(code(409, "revision_conflict"))?;
            ("task", v.task_id, result.changed, Vec::new())
        }
        Payload::LogContactAttempt(v) => {
            // D-078: this is the only mobile operation with a user-reported
            // business occurrence clock.  Sample the server clock only after
            // workspace/context/operation/Person/membership locking.
            let recorded_at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp() AS now")
                .fetch_one(&mut *tx)
                .await?;
            if v.occurred_at > recorded_at {
                return Err(code(422, "contact_time_in_future"));
            }
            let attempt = commands::log_contact_attempt_in_transaction(
                &mut tx,
                &ctx,
                LogContactAttemptInTransaction {
                    person_id: person,
                    channel: v.channel,
                    outcome: v.outcome,
                    occurred_at: v.occurred_at,
                    recorded_at: Some(recorded_at),
                },
            )
            .await
            .map_err(|error| match error {
                crate::domain::commands::CommandError::PersonNotFound => missing(),
                crate::domain::commands::CommandError::Database(error) => error.into(),
                crate::domain::commands::CommandError::Corrupt => code(503, "unavailable"),
                _ => code(503, "unavailable"),
            })?;
            ("contact_attempt", attempt.attempt.id, true, Vec::new())
        }
        Payload::ChangePersonStage(v) => {
            let result = change_person_stage_in_transaction(
                &mut tx,
                &ctx,
                ChangePersonStage {
                    person_id: person,
                    stage_id: crate::ids::StageId(v.stage_id),
                },
                Some(revision(&v.expected_stage_revision)?),
            )
            .await
            .map_err(|error| match error {
                crate::domain::commands::CommandError::PersonNotFound => missing(),
                crate::domain::commands::CommandError::InvalidStage => code(422, "invalid_stage"),
                crate::domain::commands::CommandError::Database(error) => error.into(),
                crate::domain::commands::CommandError::Corrupt => code(503, "unavailable"),
                _ => code(503, "unavailable"),
            })?
            .ok_or(code(409, "revision_conflict"))?;
            ("person_stage", person.0, result.changed, Vec::new())
        }
        Payload::UpdatePersonDetails(v) => {
            let operations = v
                .contact_operations
                .into_iter()
                .enumerate()
                .map(|(ordinal, operation)| match operation {
                    ContactOperationWire::Add { kind, value } => match kind.as_str() {
                        "email" => Ok(ContactDetailOperation::Add {
                            ordinal,
                            kind: DetailContactKind::Email,
                            value,
                        }),
                        "phone" => Ok(ContactDetailOperation::Add {
                            ordinal,
                            kind: DetailContactKind::Phone,
                            value,
                        }),
                        _ => Err(invalid()),
                    },
                    ContactOperationWire::Edit { id, value } => Ok(ContactDetailOperation::Edit {
                        id: ContactMethodId::new(id),
                        value,
                    }),
                    ContactOperationWire::Remove { id } => Ok(ContactDetailOperation::Remove {
                        id: ContactMethodId::new(id),
                    }),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let result = update_person_details_in_transaction(
                &mut tx,
                &ctx,
                UpdatePersonDetails {
                    person_id: person,
                    expected_details_revision: revision(&v.expected_details_revision)?,
                    first_name: v.first_name,
                    last_name: v.last_name,
                    contact_operations: operations,
                },
            )
            .await
            .map_err(|error| match error {
                crate::domain::commands::CommandError::PersonNotFound => missing(),
                crate::domain::commands::CommandError::InvalidPersonDetails => {
                    code(422, "invalid_person_details")
                }
                crate::domain::commands::CommandError::Database(error) => error.into(),
                _ => code(503, "unavailable"),
            })?
            .ok_or(code(409, "revision_conflict"))?;
            (
                "person_details",
                person.0,
                result.changed,
                result
                    .added_contact_ids
                    .into_iter()
                    .map(|(ordinal, id)| AddedContactId { ordinal, id: id.0 })
                    .collect(),
            )
        }
        Payload::UpdatePersonMetadata(v) => {
            let actions = v
                .actions
                .into_iter()
                .map(|action| match action {
                    MetadataAction::AddTag { tag_id } => TypedMetadataAction::AddTag {
                        tag_id: TagId::new(tag_id),
                    },
                    MetadataAction::RemoveTag { tag_id } => TypedMetadataAction::RemoveTag {
                        tag_id: TagId::new(tag_id),
                    },
                    MetadataAction::SetField { field_id, value } => TypedMetadataAction::SetField {
                        field_id: CustomFieldId::new(field_id),
                        value,
                    },
                    MetadataAction::ClearField { field_id } => TypedMetadataAction::ClearField {
                        field_id: CustomFieldId::new(field_id),
                    },
                })
                .collect();
            let result = person_metadata::update_person_metadata_in_transaction(
                &mut tx,
                &ctx,
                UpdatePersonMetadata {
                    person_id: person,
                    expected_metadata_revision: revision(&v.expected_metadata_revision)?,
                    expected_catalog_revision: revision(&v.expected_catalog_revision)?,
                    actions,
                },
            )
            .await
            .map_err(metadata_error)?;
            let changed = result.tags || result.fields;
            metadata_change = Some(result);
            ("person_metadata", person.0, changed, Vec::new())
        }
    };
    let resource_revision: Option<i64> = match kind.as_str() {
        "add_note" | "log_contact_attempt" => None,
        "change_person_stage" => Some(
            sqlx::query_scalar(
                "SELECT stage_revision FROM person WHERE organization_id=$1 AND id=$2",
            )
            .bind(auth.active_organization_id.0)
            .bind(person.0)
            .fetch_one(&mut *tx)
            .await?,
        ),
        "update_person_details" => Some(
            sqlx::query_scalar(
                "SELECT details_revision FROM person WHERE organization_id=$1 AND id=$2",
            )
            .bind(auth.active_organization_id.0)
            .bind(person.0)
            .fetch_one(&mut *tx)
            .await?,
        ),
        "update_person_metadata" => Some(
            sqlx::query_scalar(
                "SELECT metadata_revision FROM person WHERE organization_id=$1 AND id=$2",
            )
            .bind(auth.active_organization_id.0)
            .bind(person.0)
            .fetch_one(&mut *tx)
            .await?,
        ),
        "create_task" | "complete_task" | "update_task" => Some(
            sqlx::query_scalar("SELECT revision FROM task WHERE organization_id=$1 AND id=$2")
                .bind(auth.active_organization_id.0)
                .bind(resource_id)
                .fetch_one(&mut *tx)
                .await?,
        ),
        "edit_note" => Some(
            sqlx::query_scalar("SELECT revision FROM note WHERE organization_id=$1 AND id=$2")
                .bind(auth.active_organization_id.0)
                .bind(resource_id)
                .fetch_one(&mut *tx)
                .await?,
        ),
        _ => return Err(code(503, "unavailable")),
    };
    let person_revision: i64 =
        sqlx::query_scalar("SELECT mobile_revision FROM person WHERE organization_id=$1 AND id=$2")
            .bind(auth.active_organization_id.0)
            .bind(person.0)
            .fetch_one(&mut *tx)
            .await?;
    let digest = keys.digest(keys.active(), b"crm-mobile-operation-v1\0", &canonical)?;
    let accepted_at:DateTime<Utc>=sqlx::query_scalar("INSERT INTO mobile_operation_receipt(organization_id,actor_user_id,operation_id,context_id,digest_version,digest_key_id,payload_digest,kind,person_id,resource_type,resource_id,committed_revision,person_revision,changed,added_contact_ids,accepted_at) VALUES($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,statement_timestamp()) RETURNING accepted_at")
        .bind(auth.active_organization_id.0).bind(auth.actor_user_id.0).bind(request.operation_id).bind(request.context_id).bind(keys.active()).bind(digest).bind(&kind).bind(person.0).bind(resource_type).bind(resource_id).bind(resource_revision).bind(person_revision).bind(changed).bind(serde_json::to_value(&added_contact_ids).map_err(|_| invalid())?).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    if changed {
        publisher
            .publish_after_commit(Publication::for_event(RealtimeEvent::person_changed(
                auth.active_organization_id,
                Utc::now(),
                ctx.correlation_id,
                person,
                match resource_type {
                    "note" => PersonChange::NoteChanged,
                    "task" => PersonChange::TaskChanged,
                    "contact_attempt" => PersonChange::ContactAttempted,
                    "person_stage" => PersonChange::StageChanged,
                    "person_details" => PersonChange::DetailsChanged,
                    "person_metadata" if metadata_change.as_ref().is_some_and(|v| v.fields) => {
                        PersonChange::CustomFieldChanged
                    }
                    "person_metadata" => PersonChange::TagsChanged,
                    _ => return Err(code(503, "unavailable")),
                },
            )))
            .await;
        if metadata_change
            .as_ref()
            .is_some_and(|change| change.tags && change.fields)
        {
            publisher
                .publish_after_commit(Publication::for_event(RealtimeEvent::person_changed(
                    auth.active_organization_id,
                    Utc::now(),
                    ctx.correlation_id,
                    person,
                    PersonChange::TagsChanged,
                )))
                .await;
        }
    }
    Ok(Receipt {
        operation_id: request.operation_id,
        outcome: "accepted".into(),
        resource_type: resource_type.into(),
        resource_id,
        committed_revision: resource_revision.map(|r| r.to_string()),
        person_revision: person_revision.to_string(),
        changed,
        accepted_at,
        replayed: false,
        added_contact_ids: (kind == "update_person_details").then_some(added_contact_ids),
    })
}
pub async fn lookup_receipt(
    pool: &PgPool,
    auth: &AuthContext,
    context_id: Uuid,
    id: Uuid,
) -> Result<Receipt, MobileError> {
    let mut tx = begin(pool, auth, true).await?;
    context(&mut tx, auth, context_id, false).await?;
    authority(&mut tx, auth, false).await?;
    let prior = get(&mut tx, auth, id).await?.ok_or_else(missing)?;
    if prior.context != context_id {
        return Err(missing());
    }
    visible(&mut tx, auth, &prior).await?;
    tx.commit().await?;
    Ok(prior.receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_payload_preserves_explicit_null_and_operation_array_ordinals() {
        let payload = parse_details(serde_json::json!({
            "person_id":"11111111-1111-1111-1111-111111111111",
            "expected_details_revision":"7",
            "first_name":null,
            "contact_operations":[
                {"op":"edit","id":"22222222-2222-2222-2222-222222222222","value":"(555) 555-0100"},
                {"op":"add","kind":"email","value":" Ada@Example.TEST "}
            ]
        }))
        .unwrap();
        assert_eq!(payload.first_name, Some(None));
        assert_eq!(payload.last_name, None);
        assert_eq!(payload.contact_operations.len(), 2);
        assert_eq!(
            serde_json::to_value(payload).unwrap()["contact_operations"][1]["value"],
            "Ada@Example.TEST"
        );
    }

    #[test]
    fn details_payload_rejects_empty_unknown_and_oversized_changes() {
        let base = serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_details_revision":"1","contact_operations":[]});
        assert!(parse_details(base).is_err());
        assert!(parse_details(serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_details_revision":"01","last_name":"Ada","contact_operations":[]})).is_err());
        assert!(parse_details(serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_details_revision":"1","last_name":"Ada","contact_operations":[],"extra":true})).is_err());
        assert!(contact_value("x".repeat(1025)).is_err());
        assert!(clean_name("x\u{0000}".into()).is_err());
    }

    #[test]
    fn metadata_payload_requires_canonical_tokens_and_unique_targets() {
        let tag = "22222222-2222-2222-2222-222222222222";
        let field = "33333333-3333-3333-3333-333333333333";
        let value = serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_metadata_revision":"1","expected_catalog_revision":"1","actions":[{"kind":"add_tag","tag_id":tag},{"kind":"set_field","field_id":field,"value":{"number":"12.3400"}}]});
        assert!(parse_metadata(value).is_ok());
        assert!(parse_metadata(serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_metadata_revision":"01","expected_catalog_revision":"1","actions":[{"kind":"add_tag","tag_id":tag}]})).is_err());
        assert!(parse_metadata(serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_metadata_revision":"1","expected_catalog_revision":"1","actions":[{"kind":"add_tag","tag_id":tag},{"kind":"remove_tag","tag_id":tag}]})).is_err());
        assert!(parse_metadata(serde_json::json!({"person_id":"11111111-1111-1111-1111-111111111111","expected_metadata_revision":"1","expected_catalog_revision":"1","actions":[]})).is_err());
    }
}

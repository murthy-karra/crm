//! Typed, transaction-compatible Person name/contact editing for Mobile005.

use std::collections::HashSet;

use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::domain::{
    commands::CommandError,
    contact::{normalize_email, normalize_phone},
    envelope::CommandContext,
    person::queries,
};
use crate::ids::{ContactMethodId, PersonId};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailContactKind {
    Email,
    Phone,
}
impl DetailContactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Phone => "phone",
        }
    }
}

pub enum ContactDetailOperation {
    Add {
        ordinal: usize,
        kind: DetailContactKind,
        value: String,
    },
    Edit {
        id: ContactMethodId,
        value: String,
    },
    Remove {
        id: ContactMethodId,
    },
}
pub struct UpdatePersonDetails {
    pub person_id: PersonId,
    pub expected_details_revision: i64,
    pub first_name: Option<Option<String>>,
    pub last_name: Option<Option<String>>,
    pub contact_operations: Vec<ContactDetailOperation>,
}
pub struct UpdatedPersonDetails {
    pub details_revision: i64,
    pub changed: bool,
    pub added_contact_ids: Vec<(usize, ContactMethodId)>,
}
struct Contact {
    id: Uuid,
    kind: String,
    value: String,
    normalized: String,
}
fn normalize(kind: &str, value: &str) -> Result<String, CommandError> {
    match kind {
        "email" => normalize_email(value).map(|value| value.as_str().to_owned()),
        "phone" => normalize_phone(value).map(|value| value.as_str().to_owned()),
        _ => None,
    }
    .ok_or(CommandError::InvalidPersonDetails)
}

/// The mobile adapter owns the preceding intake advisory lock, receipt and
/// publication. This core locks the Person and every current contact so a
/// future conventional caller cannot create an unlocked edit path.
pub async fn update_person_details_in_transaction(
    conn: &mut PgConnection,
    ctx: &CommandContext,
    mut cmd: UpdatePersonDetails,
) -> Result<Option<UpdatedPersonDetails>, CommandError> {
    // Typed callers may use this transaction-compatible core outside the
    // mobile adapter. Reassert the workspace hold before even a stale/no-op
    // return so migration-review mode cannot be bypassed through a path that
    // happens not to issue DML.
    crate::auth::workspace::ordinary(conn, ctx.organization_id).await?;
    if cmd.expected_details_revision <= 0
        || cmd.contact_operations.len() > 50
        || cmd.contact_operations.is_empty() && cmd.first_name.is_none() && cmd.last_name.is_none()
    {
        return Err(CommandError::InvalidPersonDetails);
    }
    for name in [&mut cmd.first_name, &mut cmd.last_name] {
        if let Some(Some(value)) = name {
            *value = value.trim().to_owned();
            if value.chars().count() > 200 || value.chars().any(char::is_control) {
                return Err(CommandError::InvalidPersonDetails);
            }
        }
    }
    for operation in &mut cmd.contact_operations {
        let value = match operation {
            ContactDetailOperation::Add { value, .. }
            | ContactDetailOperation::Edit { value, .. } => value,
            ContactDetailOperation::Remove { .. } => continue,
        };
        *value = value.trim().to_owned();
        if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
            return Err(CommandError::InvalidPersonDetails);
        }
    }
    // The adapter acquires this before its common Person lock. Reassert it for
    // direct typed callers before this core takes a Person lock as well.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('intake:' || $1::text,0))")
        .bind(ctx.organization_id.to_string())
        .execute(&mut *conn)
        .await?;
    let person = queries::lock_person(conn, cmd.person_id, ctx.organization_id)
        .await?
        .ok_or(CommandError::PersonNotFound)?;
    if person.details_revision != cmd.expected_details_revision {
        return Ok(None);
    }
    let rows = sqlx::query("SELECT id,kind,value,normalized_value FROM contact_method WHERE organization_id=$1 AND person_id=$2 FOR UPDATE")
        .bind(ctx.organization_id.0).bind(cmd.person_id.0).fetch_all(&mut *conn).await?;
    let mut contacts: Vec<Contact> = rows
        .into_iter()
        .map(|row| Contact {
            id: row.get("id"),
            kind: row.get("kind"),
            value: row.get("value"),
            normalized: row.get("normalized_value"),
        })
        .collect();
    let initial_contacts = contacts
        .iter()
        .map(|contact| {
            (
                contact.id,
                (contact.kind.clone(), contact.normalized.clone()),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen_ids = HashSet::new();
    let mut mutations: Vec<(String, Uuid, Option<String>, Option<String>, Option<String>)> =
        Vec::new();
    let mut added = Vec::new();
    for op in cmd.contact_operations {
        match op {
            ContactDetailOperation::Add {
                ordinal,
                kind,
                value,
            } => {
                let normalized = normalize(kind.as_str(), &value)?;
                let id = Uuid::new_v4();
                contacts.push(Contact {
                    id,
                    kind: kind.as_str().into(),
                    value: value.clone(),
                    normalized: normalized.clone(),
                });
                mutations.push((
                    "add".into(),
                    id,
                    Some(kind.as_str().into()),
                    Some(value),
                    Some(normalized),
                ));
                added.push((ordinal, ContactMethodId::new(id)));
            }
            ContactDetailOperation::Edit { id, value } => {
                if !seen_ids.insert(id.0) {
                    return Err(CommandError::InvalidPersonDetails);
                }
                let item = contacts
                    .iter_mut()
                    .find(|item| item.id == id.0)
                    .ok_or(CommandError::InvalidPersonDetails)?;
                let normalized = normalize(&item.kind, &value)?;
                if item.value != value || item.normalized != normalized {
                    mutations.push((
                        "edit".into(),
                        id.0,
                        None,
                        Some(value.clone()),
                        Some(normalized.clone()),
                    ));
                    item.value = value;
                    item.normalized = normalized;
                }
            }
            ContactDetailOperation::Remove { id } => {
                if !seen_ids.insert(id.0) {
                    return Err(CommandError::InvalidPersonDetails);
                }
                let index = contacts
                    .iter()
                    .position(|item| item.id == id.0)
                    .ok_or(CommandError::InvalidPersonDetails)?;
                contacts.remove(index);
                mutations.push(("remove".into(), id.0, None, None, None));
            }
        }
    }
    let first = cmd.first_name.unwrap_or_else(|| person.first_name.clone());
    let last = cmd.last_name.unwrap_or_else(|| person.last_name.clone());
    if !first
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && !last
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && contacts.is_empty()
    {
        return Err(CommandError::InvalidPersonDetails);
    }
    let mut unique = HashSet::new();
    if contacts
        .iter()
        .any(|item| !unique.insert((item.kind.clone(), item.normalized.clone())))
    {
        return Err(CommandError::InvalidPersonDetails);
    }
    let names_changed = first != person.first_name || last != person.last_name;
    if !names_changed && mutations.is_empty() {
        return Ok(Some(UpdatedPersonDetails {
            details_revision: person.details_revision,
            changed: false,
            added_contact_ids: Vec::new(),
        }));
    }
    if names_changed {
        sqlx::query("UPDATE person SET first_name=$3,last_name=$4,updated_at=statement_timestamp() WHERE id=$1 AND organization_id=$2")
            .bind(cmd.person_id.0).bind(ctx.organization_id.0).bind(first).bind(last).execute(&mut *conn).await?;
    }
    // Vacate only edited normalized values needed by another edit. This keeps
    // stable contact UUIDs/creation/order and handles swaps against the existing
    // immediate unique index. Temporary values are transaction-local: rollback
    // restores them, and commit exposes only the validated final contacts.
    // The opaque aggregate revision also covers these derived intermediate writes.
    for (operation, contact_id, _, _, _) in &mutations {
        if operation != "edit" {
            continue;
        }
        let original = &initial_contacts[contact_id];
        let needed_by_edit = mutations
            .iter()
            .any(|(other_op, other_id, _, _, normalized)| {
                other_op == "edit"
                    && other_id != contact_id
                    && normalized.as_deref() == Some(original.1.as_str())
                    && initial_contacts[other_id].0 == original.0
            });
        if !needed_by_edit {
            continue;
        }
        let temporary = loop {
            let candidate = format!("crm-details-temp:{}", Uuid::new_v4());
            if !initial_contacts
                .values()
                .any(|(_, value)| value == &candidate)
                && !contacts
                    .iter()
                    .any(|contact| contact.normalized == candidate)
            {
                break candidate;
            }
        };
        sqlx::query(
            "UPDATE contact_method SET normalized_value=$3 WHERE id=$1 AND organization_id=$2",
        )
        .bind(contact_id)
        .bind(ctx.organization_id.0)
        .bind(temporary)
        .execute(&mut *conn)
        .await?;
    }
    mutations.sort_by_key(|(operation, ..)| match operation.as_str() {
        "remove" => 0_u8,
        "edit" => 1,
        "add" => 2,
        _ => 3,
    });
    for (op, id, kind, value, normalized) in mutations {
        match op.as_str() {
            "add" => {
                sqlx::query("INSERT INTO contact_method(id,organization_id,person_id,kind,value,normalized_value) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(ctx.organization_id.0).bind(cmd.person_id.0).bind(kind).bind(value).bind(normalized).execute(&mut *conn).await?;
            }
            "edit" => {
                sqlx::query("UPDATE contact_method SET value=$3,normalized_value=$4 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(value).bind(normalized).execute(&mut *conn).await?;
            }
            "remove" => {
                sqlx::query("DELETE FROM contact_method WHERE id=$1 AND organization_id=$2")
                    .bind(id)
                    .bind(ctx.organization_id.0)
                    .execute(&mut *conn)
                    .await?;
            }
            _ => return Err(CommandError::Corrupt),
        }
    }
    let details_revision: i64 = sqlx::query_scalar(
        "SELECT details_revision FROM person WHERE id=$1 AND organization_id=$2",
    )
    .bind(cmd.person_id.0)
    .bind(ctx.organization_id.0)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Some(UpdatedPersonDetails {
        details_revision,
        changed: true,
        added_contact_ids: added,
    }))
}

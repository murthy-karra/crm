//! Explicit mapping intent validation. This never creates a native destination;
//! preparation and execution separately recheck complete source and target state.
use super::{
    core_source::Derived,
    evidence::{Purpose, Scope},
    mapping_inventory::{Choice, Mapping},
    model::Family,
};
use crate::{
    config::RawPayloadKey,
    domain::{
        migration::{activity_source, MigrationError},
        task::TaskKind,
    },
    ids::OrganizationId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

#[derive(Clone, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Selection {
    Hold,
    Existing { target_id: Uuid },
    CreateMatching,
    Unassigned,
    Kind { kind: TaskKind },
}
impl<'de> Deserialize<'de> for Selection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        #[derive(Deserialize)]
        #[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Hold(Empty),
            Existing { target_id: Uuid },
            CreateMatching(Empty),
            Unassigned(Empty),
            Kind { kind: TaskKind },
        }
        Ok(match Wire::deserialize(d)? {
            Wire::Hold(_) => Self::Hold,
            Wire::Existing { target_id } => Self::Existing { target_id },
            Wire::CreateMatching(_) => Self::CreateMatching,
            Wire::Unassigned(_) => Self::Unassigned,
            Wire::Kind { kind } => Self::Kind { kind },
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPatch {
    pub mapping_id: Uuid,
    pub choice: Selection,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Destination {
    Tag {
        id: Uuid,
        label: String,
        updated_at: DateTime<Utc>,
    },
    Field {
        id: Uuid,
        label: String,
        field_type: String,
        source: Option<String>,
        external_key: Option<String>,
        updated_at: DateTime<Utc>,
    },
    Option {
        id: Uuid,
        field_id: Uuid,
        label: String,
        updated_at: DateTime<Utc>,
    },
    Member {
        id: Uuid,
        label: String,
        status: String,
    },
}
impl Destination {
    pub(super) fn id(&self) -> Uuid {
        match self {
            Self::Tag { id, .. }
            | Self::Field { id, .. }
            | Self::Option { id, .. }
            | Self::Member { id, .. } => *id,
        }
    }
}
#[derive(Serialize, Deserialize)]
pub(super) struct Patch {
    pub kind: String,
    pub source_key: Vec<u8>,
    pub source_mapping: Uuid,
    pub source_plan: Uuid,
    pub choice: Choice,
    pub destination: Option<Destination>,
}
pub(super) async fn source(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    mapping: &Mapping,
) -> Result<Derived, MigrationError> {
    let reference = mapping
        .source
        .as_ref()
        .ok_or(MigrationError::InvalidInput)?;
    let row=sqlx::query("SELECT s.nonce,s.ciphertext,s.plan_id,p.revision,p.family,p.phase FROM migration_family_refresh_source s JOIN migration_family_refresh_plan p ON p.id=s.plan_id AND p.bundle_id=s.bundle_id AND p.organization_id=s.organization_id WHERE s.id=$1 AND s.bundle_id=$2 AND s.organization_id=$3")
        .bind(reference.row).bind(scope.bundle).bind(scope.organization.0).fetch_optional(conn).await?.ok_or(MigrationError::Crypto)?;
    let family: Family = serde_json::from_value(serde_json::json!(row.get::<String, _>("family")))
        .map_err(|_| MigrationError::Crypto)?;
    if family == Family::History
        || matches!(row.get::<String, _>("phase").as_str(), "cohort" | "capture")
    {
        return Err(MigrationError::Crypto);
    }
    let source_scope = Scope {
        plan: row.get("plan_id"),
        revision: row.get("revision"),
        family,
        ..scope
    };
    source_scope.open(
        key,
        reference.row,
        Purpose::Source,
        row.get("nonce"),
        row.get("ciphertext"),
    )
}

pub(super) async fn validate(
    conn: &mut PgConnection,
    org: OrganizationId,
    mapping: &Mapping,
    source: &Derived,
    selection: &Selection,
    parent_target: Option<Uuid>,
) -> Result<(Choice, Option<Destination>), MigrationError> {
    if matches!(selection, Selection::Hold) {
        return Ok((Choice::Hold, None));
    }
    if !mapping.qualified {
        return Err(MigrationError::InvalidImportChoice);
    }
    match selection {
        Selection::CreateMatching
            if matches!(mapping.kind.as_str(), "tag" | "field" | "option")
                && mapping.creation_allowed
                && (mapping.kind != "option" || parent_target.is_some()) =>
        {
            Ok((
                Choice::CreateMatching {
                    target: Uuid::new_v4(),
                },
                None,
            ))
        }
        Selection::Unassigned
            if matches!(
                mapping.kind.as_str(),
                "note_author" | "task_creator" | "task_assignee"
            ) =>
        {
            Ok((Choice::Unassigned, None))
        }
        Selection::Kind { kind } if mapping.kind == "task_kind" => {
            Ok((Choice::Kind { kind: *kind }, None))
        }
        Selection::Existing { target_id } => {
            let target = *target_id;
            // These are intent snapshots, not native application locks. Holding
            // catalog row locks behind retention locks would invert first-import
            // writers. Classification/confirmation/application recheck them.
            let destination = match mapping.kind.as_str() {
                "tag" => {
                    let r = sqlx::query(
                        "SELECT id,name,updated_at FROM tag WHERE id=$1 AND organization_id=$2",
                    )
                    .bind(target)
                    .bind(org.0)
                    .fetch_optional(&mut *conn)
                    .await?
                    .ok_or(MigrationError::InvalidImportChoice)?;
                    Destination::Tag {
                        id: target,
                        label: r.get("name"),
                        updated_at: r.get("updated_at"),
                    }
                }
                "field" => {
                    let Derived::Metadata(record) = source else {
                        return Err(MigrationError::Crypto);
                    };
                    let crate::domain::migration::metadata_source::Entity::Field(field) =
                        &record.entity
                    else {
                        return Err(MigrationError::Crypto);
                    };
                    let r=sqlx::query("SELECT id,label,field_type,source,external_key,updated_at FROM custom_field WHERE id=$1 AND organization_id=$2 AND archived_at IS NULL").bind(target).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::InvalidImportChoice)?;
                    let field_type: String = r.get("field_type");
                    let native_source: Option<String> = r.get("source");
                    let external_key: Option<String> = r.get("external_key");
                    if field.field_type.as_deref() != Some(&field_type)
                        || (native_source.is_some()
                            && (native_source.as_deref() != Some("fub")
                                || external_key != field.name))
                    {
                        return Err(MigrationError::InvalidImportChoice);
                    }
                    Destination::Field {
                        id: target,
                        label: r.get("label"),
                        field_type,
                        source: native_source,
                        external_key,
                        updated_at: r.get("updated_at"),
                    }
                }
                "option" => {
                    let r=sqlx::query("SELECT o.id,o.field_id,o.label,o.updated_at FROM custom_field_option o JOIN custom_field f ON f.id=o.field_id AND f.organization_id=o.organization_id WHERE o.id=$1 AND o.organization_id=$2 AND o.archived_at IS NULL AND f.archived_at IS NULL AND f.field_type='choice'").bind(target).bind(org.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::InvalidImportChoice)?;
                    let field_id: Uuid = r.get("field_id");
                    if Some(field_id) != parent_target {
                        return Err(MigrationError::InvalidImportChoice);
                    }
                    Destination::Option {
                        id: target,
                        field_id,
                        label: r.get("label"),
                        updated_at: r.get("updated_at"),
                    }
                }
                "note_author" | "task_creator" | "task_assignee" => {
                    let r=sqlx::query("SELECT m.status,u.display_name FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.user_id=$2").bind(org.0).bind(target).fetch_optional(&mut *conn).await?.ok_or(MigrationError::InvalidImportChoice)?;
                    let status: String = r.get("status");
                    if mapping.kind == "task_assignee" && status != "active" {
                        return Err(MigrationError::InvalidImportChoice);
                    }
                    Destination::Member {
                        id: target,
                        label: r.get("display_name"),
                        status,
                    }
                }
                _ => return Err(MigrationError::InvalidImportChoice),
            };
            Ok((Choice::Existing { target }, Some(destination)))
        }
        _ => Err(MigrationError::InvalidImportChoice),
    }
}
pub(super) fn timezone(zone: Option<String>) -> Result<Choice, MigrationError> {
    match zone {
        Some(zone) if activity_source::valid_timezone(&zone) => Ok(Choice::Timezone { zone }),
        None => Ok(Choice::Hold),
        _ => Err(MigrationError::InvalidInput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn choices_cannot_supply_hidden_targets_or_native_rows() {
        for value in [
            json!({"action":"hold","target_id":Uuid::new_v4()}),
            json!({"action":"create_matching","target_id":Uuid::new_v4()}),
            json!({"action":"unassigned","actor":Uuid::new_v4()}),
            json!({"action":"existing","target_id":Uuid::new_v4(),"organization_id":Uuid::new_v4()}),
        ] {
            assert!(serde_json::from_value::<Selection>(value).is_err());
        }
        assert!(serde_json::from_value::<Selection>(json!({"action":"create_matching"})).is_ok());
        assert!(timezone(Some("America/Los_Angeles".into())).is_ok());
        assert!(timezone(Some("user-provided-offset".into())).is_err());
    }
}

use serde_json::Value;
use sqlx::PgConnection;

use crate::domain::admin::Role;
use crate::ids::{CustomFieldId, OrganizationId, PersonId, UserId};

pub async fn rebuild(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT crm_rebuild_person_detail_projection($1,$2)")
        .bind(organization_id.0)
        .bind(person_id.0)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn load(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Option<Value>, sqlx::Error> {
    let mut snapshot = sqlx::query_scalar::<_, Value>(
        "SELECT snapshot FROM person_detail_projection WHERE organization_id=$1 AND person_id=$2",
    )
    .bind(organization_id.0)
    .bind(person_id.0)
    .fetch_optional(conn)
    .await?;
    if let Some(value) = snapshot.as_mut() {
        super::normalize_json_timestamps(value);
    }
    Ok(snapshot)
}

pub async fn rebuild_for_custom_field(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    field_id: CustomFieldId,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT crm_rebuild_person_detail_projection(organization_id,person_id) FROM person_custom_field_value WHERE organization_id=$1 AND field_id=$2 ORDER BY person_id",
    )
    .bind(organization_id.0)
    .bind(field_id.0)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn rebuild_for_custom_field_order(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT crm_rebuild_person_detail_projection(organization_id,person_id) FROM (SELECT DISTINCT organization_id,person_id FROM person_custom_field_value WHERE organization_id=$1) affected ORDER BY person_id",
    )
    .bind(organization_id.0)
    .execute(conn)
    .await?;
    Ok(())
}

/// `can_manage` depends on the authenticated viewer and therefore cannot be
/// persisted in the Organization-wide projection.
pub fn decorate_for_viewer(snapshot: &mut Value, viewer_id: UserId, role: Role) {
    let viewer_id = viewer_id.to_string();
    if let Some(tasks) = snapshot.get_mut("tasks").and_then(Value::as_array_mut) {
        for task in tasks {
            let owns_task = ["assignee", "created_by"].iter().any(|field| {
                task.get(*field)
                    .and_then(|value| value.get("id"))
                    .and_then(Value::as_str)
                    .is_some_and(|id| id == viewer_id)
            });
            task["can_manage"] = Value::Bool(role == Role::Admin || owns_task);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn task_management_is_derived_per_viewer() {
        let creator = UserId::new(Uuid::new_v4());
        let assignee = UserId::new(Uuid::new_v4());
        let outsider = UserId::new(Uuid::new_v4());
        let task = || {
            serde_json::json!({
                "assignee": {"id": assignee, "display_name": "Assignee"},
                "created_by": {"id": creator, "display_name": "Creator"}
            })
        };

        for (viewer, role, expected) in [
            (creator, Role::Member, true),
            (assignee, Role::Member, true),
            (outsider, Role::Member, false),
            (outsider, Role::Admin, true),
        ] {
            let mut snapshot = serde_json::json!({"tasks": [task()]});
            decorate_for_viewer(&mut snapshot, viewer, role);
            assert_eq!(snapshot["tasks"][0]["can_manage"], expected);
        }
    }
}

//! Reconstruct B from authenticated first-import evidence, never from C.
use super::activity_baseline::AfterState;
use crate::domain::migration::{
    activity_model, activity_source::NativeActivity, activity_store, admitted_activity_model,
    admitted_activity_store, MigrationError,
};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgConnection, Row};
use uuid::Uuid;

pub(super) async fn reconstruct(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    row: &PgRow,
    admitted: bool,
    result_native: &Value,
) -> Result<Option<AfterState>, MigrationError> {
    let kind: String = row.get("kind");
    let installed: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT crm_family_refresh_revision_installed($1)")
            .bind(&kind)
            .fetch_one(&mut *conn)
            .await?;
    if !installed.is_some_and(|at| row.get::<chrono::DateTime<chrono::Utc>, _>("committed_at") > at)
    {
        return Ok(None);
    }
    let (native, native_kind) = if admitted {
        let frozen: admitted_activity_model::Manifest = admitted_activity_store::open(
            key,
            org,
            row.get("snapshot_id"),
            row.get("plan_id"),
            row.get("manifest_id"),
            "manifest",
            row.get("manifest_nonce"),
            row.get("manifest_ciphertext"),
        )?;
        (frozen.native, frozen.native_kind)
    } else {
        let frozen: activity_model::Manifest = activity_store::open(
            key,
            org,
            row.get("snapshot_id"),
            row.get("plan_id"),
            row.get("manifest_id"),
            "manifest",
            row.get("manifest_nonce"),
            row.get("manifest_ciphertext"),
        )?;
        (frozen.native, frozen.native_kind)
    };
    let target: Uuid = row.get("target_id");
    if result_native != &json!({"target_id": target,"native_kind":native_kind,"preview":native}) {
        return Ok(None);
    }
    let mut expected = json!({
        "id":target,"organization_id":org.0,"person_id":row.get::<Uuid,_>("person_id"),
        "correlation_id":row.get::<Uuid,_>("import_id"),"origin":"migration","source":"fub",
        "source_external_id":row.get::<String,_>("native_source_key"),"revision":1,
        "deleted_at":null,"deleted_by_user_id":null
    });
    let query = match native {
        Some(NativeActivity::Note {
            body,
            created_at,
            updated_at,
            ..
        }) if kind == "note" => {
            expected["body"] = json!(body);
            expected["author_user_id"] = json!(row.get::<Option<Uuid>, _>("author_user_id"));
            expected["created_at"] = json!(created_at);
            expected["updated_at"] = json!(updated_at);
            "SELECT to_jsonb(jsonb_populate_record(NULL::note,$1))"
        }
        Some(NativeActivity::Task {
            title,
            created_at,
            updated_at,
            due_at,
            completed_at,
            ..
        }) if kind == "task" => {
            let Some(native_kind) = native_kind else {
                return Ok(None);
            };
            expected["title"] = json!(title);
            expected["kind"] = json!(native_kind);
            expected["created_by_user_id"] = json!(row.get::<Option<Uuid>, _>("creator_user_id"));
            expected["assignee_user_id"] = json!(row.get::<Option<Uuid>, _>("assignee_user_id"));
            expected["created_at"] = json!(created_at);
            expected["updated_at"] = json!(updated_at);
            expected["due_at"] = json!(due_at);
            expected["completed_at"] = json!(completed_at);
            expected["completed_by_user_id"] = Value::Null;
            "SELECT to_jsonb(jsonb_populate_record(NULL::task,$1))"
        }
        _ => return Ok(None),
    };
    // Normalize PostgreSQL timestamps/NULL columns using the row type, without
    // reading a mutable row or copying any observed value into the baseline.
    let native = sqlx::query_scalar(query)
        .bind(expected)
        .fetch_one(conn)
        .await?;
    Ok(Some(AfterState { version: 1, native }))
}

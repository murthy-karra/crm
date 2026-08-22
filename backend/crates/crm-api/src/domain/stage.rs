//! Person stages (D-019): a per-Organization list, not a fixed enum.
//! `crm_app` has SELECT only — `seed_defaults` is a library helper used by
//! the seed binary and test fixtures, never by the application
//! (docs/specs/SLICE_002.md §2).

use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// Follow Up Boss's nine defaults, in D-019 order.
pub const DEFAULT_STAGE_NAMES: [&str; 9] = [
    "Lead",
    "Hot Prospect",
    "Nurture",
    "Active Client",
    "Pending",
    "Closed",
    "Past Client",
    "Sphere",
    "Trash",
];

#[derive(Debug, Clone, Serialize)]
pub struct Stage {
    pub id: Uuid,
    pub name: String,
    pub position: i16,
}

/// Idempotent on `name`: inserts any of the nine D-019 default stages for
/// `organization_id` that do not already exist. Callers run this inside
/// their own transaction.
pub async fn seed_defaults(
    tx: &mut PgConnection,
    organization_id: Uuid,
) -> Result<(), sqlx::Error> {
    for (index, name) in DEFAULT_STAGE_NAMES.iter().enumerate() {
        let position = (index + 1) as i16;
        sqlx::query!(
            r#"INSERT INTO stage (organization_id, name, position)
               VALUES ($1, $2, $3)
               ON CONFLICT (organization_id, name) DO NOTHING"#,
            organization_id,
            *name,
            position,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// All stages for `organization_id`, in position order
/// (`GET /api/stages`; docs/specs/SLICE_002.md §5).
pub async fn list(
    conn: &mut PgConnection,
    organization_id: Uuid,
) -> Result<Vec<Stage>, sqlx::Error> {
    sqlx::query_as!(
        Stage,
        r#"SELECT id, name, position FROM stage WHERE organization_id = $1 ORDER BY position"#,
        organization_id,
    )
    .fetch_all(conn)
    .await
}

/// The Organization's position-1 stage id, used to place a newly-created
/// Person on intake. `None` means the Organization has no stages — a
/// misconfiguration (spec §9: seed and fixtures always create them).
pub async fn first_id(
    conn: &mut PgConnection,
    organization_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1"#,
        organization_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.id))
}

/// Whether `stage_id` belongs to `organization_id` — used to validate
/// `ChangePersonStage` (spec §4, §6: identical `invalid_stage` for
/// nonexistent and other-Organization ids).
pub async fn exists(
    conn: &mut PgConnection,
    stage_id: Uuid,
    organization_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM stage WHERE id = $1 AND organization_id = $2"#,
        stage_id,
        organization_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

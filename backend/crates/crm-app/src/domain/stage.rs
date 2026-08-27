//! Person stages (D-019): a per-Organization list, not a fixed enum.
//! `crm_app` has SELECT and INSERT (amended by docs/specs/SLICE_004.md §2,
//! declared change, AGENTS.md §11) — `seed_defaults` is a library helper
//! called from the application path by `CreateOrganization`
//! (domain/admin/commands/create_organization.rs), and also used directly
//! by test fixtures and the `crm-admin` CLI. The SLICE_002 §2 "never by the
//! application" restriction is superseded.

use serde::Serialize;
use sqlx::PgConnection;

use crate::ids::{OrganizationId, StageId};

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
    pub id: StageId,
    pub name: String,
    pub position: i16,
}

/// The direct `query_as!` decode target for `list` — bare `Uuid` per the
/// sqlx strategy (private row-boundary struct; `Stage` itself carries the
/// typed id).
struct StageRow {
    id: uuid::Uuid,
    name: String,
    position: i16,
}

impl From<StageRow> for Stage {
    fn from(row: StageRow) -> Self {
        Stage {
            id: StageId::new(row.id),
            name: row.name,
            position: row.position,
        }
    }
}

/// Idempotent on `name`: inserts any of the nine D-019 default stages for
/// `organization_id` that do not already exist. Callers run this inside
/// their own transaction.
pub async fn seed_defaults(
    tx: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(), sqlx::Error> {
    for (index, name) in DEFAULT_STAGE_NAMES.iter().enumerate() {
        let position = (index + 1) as i16;
        sqlx::query!(
            r#"INSERT INTO stage (organization_id, name, position)
               VALUES ($1, $2, $3)
               ON CONFLICT (organization_id, name) DO NOTHING"#,
            organization_id.0,
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
    organization_id: OrganizationId,
) -> Result<Vec<Stage>, sqlx::Error> {
    let rows = sqlx::query_as!(
        StageRow,
        r#"SELECT id, name, position FROM stage WHERE organization_id = $1 ORDER BY position"#,
        organization_id.0,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(Stage::from).collect())
}

/// The Organization's position-1 stage id, used to place a newly-created
/// Person on intake. `None` means the Organization has no stages — a
/// misconfiguration (spec §9: seed and fixtures always create them).
pub async fn first_id(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<Option<StageId>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id FROM stage WHERE organization_id = $1 ORDER BY position LIMIT 1"#,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| StageId::new(r.id)))
}

/// Whether `stage_id` belongs to `organization_id` — used to validate
/// `ChangePersonStage` (spec §4, §6: identical `invalid_stage` for
/// nonexistent and other-Organization ids).
pub async fn exists(
    conn: &mut PgConnection,
    stage_id: StageId,
    organization_id: OrganizationId,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 as "present!" FROM stage WHERE id = $1 AND organization_id = $2"#,
        stage_id.0,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

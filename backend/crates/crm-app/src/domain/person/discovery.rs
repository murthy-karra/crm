//! Bounded native discovery. Hits are previews, never complete offline bundles.
use serde::Serialize;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    model::{compute_display_name, StageRef, UserRef},
    queries::escape_like,
    PersonVisibilityScope,
};
use crate::{
    domain::contact::{normalize_email, normalize_phone},
    ids::{PersonId, StageId, UserId},
};

pub const RESULT_LIMIT: usize = 25;

#[derive(Serialize)]
pub struct DiscoveryItem {
    pub person_id: PersonId,
    pub display_name: String,
    pub stage: StageRef,
    pub assigned_user: Option<UserRef>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
}

#[derive(FromRow)]
struct DiscoveryRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    stage_id: Uuid,
    stage_name: String,
    assigned_user_id: Option<Uuid>,
    assigned_user_display_name: Option<String>,
    primary_email: Option<String>,
    primary_phone: Option<String>,
}

/// Exposed so plan evidence runs the exact production statement.
pub const DISCOVERY_SQL: &str = r#"
SELECT p.id, p.first_name, p.last_name, s.id AS stage_id, s.name AS stage_name,
       u.id AS assigned_user_id, u.display_name AS assigned_user_display_name,
       (SELECT cm.value FROM contact_method cm
        WHERE cm.person_id=p.id AND cm.organization_id=p.organization_id AND cm.kind='email'
        ORDER BY cm.import_order ASC NULLS LAST, cm.created_at ASC, cm.id ASC LIMIT 1) AS primary_email,
       (SELECT cm.value FROM contact_method cm
        WHERE cm.person_id=p.id AND cm.organization_id=p.organization_id AND cm.kind='phone'
        ORDER BY cm.import_order ASC NULLS LAST, cm.created_at ASC, cm.id ASC LIMIT 1) AS primary_phone
FROM person p
JOIN stage s ON s.id=p.stage_id AND s.organization_id=p.organization_id
LEFT JOIN app_user u ON u.id=p.assigned_user_id
WHERE p.organization_id=$1 AND (
    p.first_name ILIKE $2 ESCAPE '\'
    OR p.last_name ILIKE $2 ESCAPE '\'
    OR concat_ws(' ',p.first_name,p.last_name) ILIKE $2 ESCAPE '\'
    OR EXISTS (
        SELECT 1 FROM contact_method cm
        WHERE cm.person_id=p.id AND cm.organization_id=p.organization_id
          AND ((cm.kind='email' AND cm.normalized_value=$3)
            OR (cm.kind='phone' AND cm.normalized_value=$4))
    )
)
ORDER BY p.last_name ASC NULLS LAST, p.first_name ASC NULLS LAST, p.id ASC
LIMIT 26
"#;

pub async fn search(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    term: &str,
) -> Result<(Vec<DiscoveryItem>, bool), sqlx::Error> {
    let mut reader = crate::auth::workspace::read(conn, scope.organization_id()).await?;
    let pattern = format!("%{}%", escape_like(term));
    let email = normalize_email(term);
    let phone = normalize_phone(term);
    let mut rows = sqlx::query_as::<_, DiscoveryRow>(DISCOVERY_SQL)
        .bind(scope.organization_id().0)
        .bind(pattern)
        .bind(email.as_ref().map(|v| v.as_str()))
        .bind(phone.as_ref().map(|v| v.as_str()))
        .fetch_all(&mut *reader)
        .await?;
    let has_more = rows.len() > RESULT_LIMIT;
    rows.truncate(RESULT_LIMIT);
    let items = rows
        .into_iter()
        .map(|row| DiscoveryItem {
            person_id: PersonId::new(row.id),
            display_name: compute_display_name(
                row.first_name.as_deref(),
                row.last_name.as_deref(),
                row.primary_email.as_deref(),
                row.primary_phone.as_deref(),
            ),
            stage: StageRef {
                id: StageId::new(row.stage_id),
                name: row.stage_name,
            },
            assigned_user: row
                .assigned_user_id
                .zip(row.assigned_user_display_name)
                .map(|(id, display_name)| UserRef {
                    id: UserId::new(id),
                    display_name,
                }),
            primary_email: row.primary_email,
            primary_phone: row.primary_phone,
        })
        .collect();
    Ok((items, has_more))
}

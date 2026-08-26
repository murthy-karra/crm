//! `inquiry` persistence and read queries (docs/specs/SLICE_002.md §2, §5).

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::ids::OrganizationId;

pub struct NewInquiry<'a> {
    pub organization_id: OrganizationId,
    pub person_id: Uuid,
    pub raw_payload_id: Uuid,
    pub source: &'a str,
    pub source_external_id: Option<&'a str>,
    pub message: Option<&'a str>,
    pub received_at: DateTime<Utc>,
}

pub async fn insert(conn: &mut PgConnection, new: NewInquiry<'_>) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO inquiry
            (organization_id, person_id, raw_payload_id, source, source_external_id, message, received_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id"#,
        new.organization_id.0,
        new.person_id,
        new.raw_payload_id,
        new.source,
        new.source_external_id,
        new.message,
        new.received_at,
    )
    .fetch_one(conn)
    .await?;
    Ok(row.id)
}

#[derive(Debug, Clone, Serialize)]
pub struct InquirySummary {
    pub id: Uuid,
    pub source: String,
    pub source_external_id: Option<String>,
    pub message: Option<String>,
    pub received_at: DateTime<Utc>,
}

/// A Person's Inquiries, `received_at DESC` (`GET /api/people/{id}`; spec
/// §5). Caller is responsible for the `PersonVisibilityScope` check.
pub async fn list_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: Uuid,
) -> Result<Vec<InquirySummary>, sqlx::Error> {
    sqlx::query_as!(
        InquirySummary,
        r#"SELECT id, source, source_external_id, message, received_at
           FROM inquiry
           WHERE organization_id = $1 AND person_id = $2
           ORDER BY received_at DESC"#,
        organization_id.0,
        person_id,
    )
    .fetch_all(conn)
    .await
}

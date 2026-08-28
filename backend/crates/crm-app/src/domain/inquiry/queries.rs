//! `inquiry` persistence and read queries (docs/specs/SLICE_002.md §2, §5;
//! docs/specs/SLICE_011a.md §5b).

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;

use crate::ids::{InquiryId, OrganizationId, PersonId, RawPayloadId};

pub struct NewInquiry<'a> {
    pub organization_id: OrganizationId,
    pub person_id: PersonId,
    pub raw_payload_id: RawPayloadId,
    pub source: &'a str,
    pub source_external_id: Option<&'a str>,
    pub message: Option<&'a str>,
    pub received_at: DateTime<Utc>,
}

pub async fn insert(
    conn: &mut PgConnection,
    new: NewInquiry<'_>,
) -> Result<InquiryId, sqlx::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO inquiry
            (organization_id, person_id, raw_payload_id, source, source_external_id, message, received_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING id"#,
        new.organization_id.0,
        new.person_id.0,
        new.raw_payload_id.0,
        new.source,
        new.source_external_id,
        new.message,
        new.received_at,
    )
    .fetch_one(conn)
    .await?;
    Ok(InquiryId::new(row.id))
}

/// The direct `query_as!` decode target is this struct itself — `id` stays
/// the bare row value at construction and is wrapped below at the public
/// boundary (sqlx strategy: row structs stay bare `Uuid`, but
/// `InquirySummary` is the returned public struct, not a private mapping
/// intermediary, so its own `id` field is typed).
#[derive(Debug, Clone, Serialize)]
pub struct InquirySummary {
    pub id: InquiryId,
    pub source: String,
    pub source_external_id: Option<String>,
    pub message: Option<String>,
    pub received_at: DateTime<Utc>,
}

struct InquirySummaryRow {
    id: uuid::Uuid,
    source: String,
    source_external_id: Option<String>,
    message: Option<String>,
    received_at: DateTime<Utc>,
}

impl From<InquirySummaryRow> for InquirySummary {
    fn from(row: InquirySummaryRow) -> Self {
        InquirySummary {
            id: InquiryId::new(row.id),
            source: row.source,
            source_external_id: row.source_external_id,
            message: row.message,
            received_at: row.received_at,
        }
    }
}

/// A Person's Inquiries, `received_at DESC` (`GET /api/people/{id}`; spec
/// §5). Caller is responsible for the `PersonVisibilityScope` check.
pub async fn list_for_person(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: PersonId,
) -> Result<Vec<InquirySummary>, sqlx::Error> {
    let rows = sqlx::query_as!(
        InquirySummaryRow,
        r#"SELECT id, source, source_external_id, message, received_at
           FROM inquiry
           WHERE organization_id = $1 AND person_id = $2
           ORDER BY received_at DESC"#,
        organization_id.0,
        person_id.0,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(InquirySummary::from).collect())
}

/// `GET /api/inquiry-sources` (docs/specs/SLICE_011a.md §5b): the
/// Organization's distinct inquiry sources, ascending, capped at 500 with
/// the house truncate-to-500 math. Org from session only (`auth.
/// active_organization_id`) — Organization data, not Person visibility
/// (the `GET /api/stages` pattern), so this takes `organization_id`
/// directly rather than a `PersonVisibilityScope`.
pub async fn distinct_sources(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
) -> Result<(Vec<String>, bool), sqlx::Error> {
    let mut rows = sqlx::query_scalar!(
        r#"SELECT DISTINCT source FROM inquiry WHERE organization_id = $1 ORDER BY source ASC LIMIT 501"#,
        organization_id.0,
    )
    .fetch_all(conn)
    .await?;
    let truncated = rows.len() > 500;
    rows.truncate(500);
    Ok((rows, truncated))
}

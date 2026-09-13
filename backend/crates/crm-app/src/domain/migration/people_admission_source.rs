//! Source qualification for new-Person admission. A report label cannot prove
//! absence. First qualify each complete retained stream once, one capture per
//! checkpoint; only then use the immutable exact-ID index for candidate lookups.
use super::{
    crypto,
    import_source::{self, ExtractedRecord},
    people_admission_store::CAPTURE_LIMIT,
    snapshot,
    snapshot_source::Stream,
    MigrationError,
};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

const SCHEMA: &str = "sha256:e136daf321fae96c7feb895fe662e0e925c501e059b8fd0faff6074cccff5c79";
const MAX_CAPTURE: i64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationCursor {
    pub sequence: i64,
    pub accepted_captures: i64,
    pub records: i64,
}
pub struct QualificationPage {
    pub cursor: QualificationCursor,
    pub complete: bool,
    pub raw_bytes: i64,
}

pub struct RetainedRecord {
    pub record: ExtractedRecord,
    pub capture_id: Uuid,
    pub ordinal: i32,
    pub raw_bytes: i64,
}
pub enum Observation {
    /// This is a point-index absence, valid as source absence only after the
    /// entire original stream has passed qualify_stream_page to exhaustion.
    Absent,
    Qualified(Box<RetainedRecord>),
    Unqualified(&'static str),
}

pub fn valid_source_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0] != b'0'
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

// Each frozen source boundary has its own cursor. The caller persists the
// returned cursor atomically with its preparation phase and owned byte charge.
// No capture is re-read for each candidate; all traversal uses its sequence key.
#[allow(clippy::too_many_arguments)]
pub async fn qualify_stream_page(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot_id: Uuid,
    source_account_id: i64,
    stream: Stream,
    final_sequence: i64,
    mut cursor: QualificationCursor,
) -> Result<QualificationPage, MigrationError> {
    if !matches!(stream, Stream::People | Stream::Users | Stream::Stages)
        || cursor.sequence < 0
        || cursor.sequence > final_sequence
        || cursor.accepted_captures < 0
        || cursor.records < 0
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let boundary = sqlx::query(
        "SELECT s.source_account_id,s.profile_version,s.schema_version,s.state,s.capture_sequence,
                s.started_at,s.completed_at,t.state AS stream_state,t.accepted_captures,t.returned_items
           FROM migration_snapshot s JOIN migration_snapshot_stream t
             ON t.snapshot_id=s.id AND t.organization_id=s.organization_id
          WHERE s.id=$1 AND s.organization_id=$2 AND t.stream=$3"
    ).bind(snapshot_id).bind(org.0).bind(stream.as_str()).fetch_optional(&mut *conn).await?
        .ok_or(MigrationError::SourceNotEligible)?;
    if boundary.get::<i64, _>("source_account_id") != source_account_id
        || boundary.get::<String, _>("profile_version") != snapshot::PROFILE
        || boundary.get::<String, _>("schema_version") != SCHEMA
        || !matches!(
            boundary.get::<String, _>("state").as_str(),
            "completed" | "completed_with_gaps"
        )
        || boundary.get::<i64, _>("capture_sequence") != final_sequence
        || boundary.get::<String, _>("stream_state") != "completed"
        || boundary
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at")
            .is_none()
        || boundary
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at")
            .is_none()
    {
        return Err(MigrationError::SourceNotEligible);
    }
    // Only a small metadata descriptor is selected until its raw/encrypted
    // lengths have passed admission. Never select ciphertext into this page.
    let descriptor = sqlx::query(
        "SELECT id,sequence,raw_byte_len,octet_length(ciphertext)::bigint AS encrypted_bytes,
                octet_length(nonce) AS nonce_bytes,accepted,truncated,http_status,classification,representation
           FROM migration_snapshot_capture
          WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4
            AND stream=$5 ORDER BY sequence LIMIT 1"
    ).bind(snapshot_id).bind(org.0).bind(cursor.sequence).bind(final_sequence).bind(stream.as_str())
        .fetch_optional(&mut *conn).await?;
    let Some(descriptor) = descriptor else {
        if cursor.accepted_captures != boundary.get::<i64, _>("accepted_captures")
            || cursor.records != boundary.get::<i64, _>("returned_items")
            || cursor.accepted_captures == 0
        {
            return Err(MigrationError::SourceNotEligible);
        }
        return Ok(QualificationPage {
            cursor,
            complete: true,
            raw_bytes: 0,
        });
    };
    if !capture_qualified(&descriptor, stream) {
        return Err(MigrationError::SourceNotEligible);
    }
    let capture_id = descriptor.get("id");
    let raw = open_capture(conn, key, org, snapshot_id, capture_id, &descriptor).await?;
    let records =
        import_source::extract_page(stream, &raw).map_err(|_| MigrationError::SourceNotEligible)?;
    let index = sqlx::query(
        "SELECT source_id,ordinal,family,representation,semantic_hmac,capture_sequence
           FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3
          ORDER BY ordinal LIMIT 101"
    ).bind(capture_id).bind(snapshot_id).bind(org.0).fetch_all(&mut *conn).await?;
    if records.len() != index.len() || index.len() > 100 {
        return Err(MigrationError::SourceNotEligible);
    }
    for (ordinal, (record, stored)) in records.iter().zip(index.iter()).enumerate() {
        let source_id = record
            .source_id
            .as_deref()
            .ok_or(MigrationError::SourceNotEligible)?;
        if !valid_source_id(source_id)
            || stored.get::<i32, _>("ordinal") != ordinal as i32
            || stored.get::<Option<String>, _>("source_id").as_deref() != Some(source_id)
            || stored.get::<String, _>("family") != stream.family().as_str()
            || stored.get::<String, _>("representation") != stream.representation()
            || stored.get::<i64, _>("capture_sequence") != descriptor.get::<i64, _>("sequence")
            || stored.get::<Vec<u8>, _>("semantic_hmac")
                != crypto::snapshot_hmac(
                    key,
                    org,
                    &format!("semantic:{}", stream.representation()),
                    &record.canonical,
                )
        {
            return Err(MigrationError::SourceNotEligible);
        }
    }
    cursor.sequence = descriptor.get("sequence");
    cursor.accepted_captures += 1;
    cursor.records += records.len() as i64;
    Ok(QualificationPage {
        cursor,
        complete: false,
        raw_bytes: raw.len() as i64,
    })
}

fn capture_qualified(row: &sqlx::postgres::PgRow, stream: Stream) -> bool {
    let raw: i64 = row.get("raw_byte_len");
    (0..=MAX_CAPTURE).contains(&raw)
        && row.get::<i64, _>("encrypted_bytes") == raw + 16
        && row.get::<i32, _>("nonce_bytes") == 24
        && row.get::<bool, _>("accepted")
        && !row.get::<bool, _>("truncated")
        && (200..300).contains(&row.get::<i32, _>("http_status"))
        && row.get::<String, _>("classification") == "success"
        && row.get::<String, _>("representation") == stream.representation()
}
async fn open_capture(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot_id: Uuid,
    capture_id: Uuid,
    descriptor: &sqlx::postgres::PgRow,
) -> Result<Vec<u8>, MigrationError> {
    let row=sqlx::query("SELECT nonce,ciphertext FROM migration_snapshot_capture WHERE id=$1 AND snapshot_id=$2 AND organization_id=$3 AND octet_length(ciphertext)=$4")
        .bind(capture_id).bind(snapshot_id).bind(org.0).bind(descriptor.get::<i64,_>("encrypted_bytes"))
        .fetch_optional(conn).await?.ok_or(MigrationError::SourceNotEligible)?;
    let raw = crypto::open_snapshot(
        key,
        org,
        snapshot_id,
        capture_id,
        "capture",
        &row.get::<Vec<u8>, _>("nonce"),
        &row.get::<Vec<u8>, _>("ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)?;
    if raw.len() as i64 != descriptor.get::<i64, _>("raw_byte_len") {
        return Err(MigrationError::Crypto);
    }
    Ok(raw)
}

/// Bounded point lookup of all observations for an exact identity. Unsafe or
/// conflicting observations are explicit and never collapse to Absent.
#[allow(clippy::too_many_arguments)]
pub async fn retained_record(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot_id: Uuid,
    stream: Stream,
    final_sequence: i64,
    source_id: &str,
) -> Result<Observation, MigrationError> {
    if !valid_source_id(source_id) {
        return Ok(Observation::Unqualified("invalid_source_id"));
    }
    let rows=sqlx::query(
        "SELECT r.source_id,r.capture_id,r.ordinal,r.semantic_hmac,r.representation AS record_representation,
                r.capture_sequence,c.sequence,c.raw_byte_len,c.accepted,c.truncated,c.http_status,
                c.classification,c.representation,octet_length(c.ciphertext)::bigint AS encrypted_bytes,
                octet_length(c.nonce) AS nonce_bytes,c.stream
           FROM migration_snapshot_record r JOIN migration_snapshot_capture c
             ON c.id=r.capture_id AND c.snapshot_id=r.snapshot_id AND c.organization_id=r.organization_id
          WHERE r.snapshot_id=$1 AND r.organization_id=$2 AND r.family=$3 AND r.source_id=$4
            AND r.capture_sequence<=$5 ORDER BY r.capture_sequence,r.ordinal LIMIT 51"
    ).bind(snapshot_id).bind(org.0).bind(stream.family().as_str()).bind(source_id).bind(final_sequence)
        .fetch_all(&mut *conn).await?;
    if rows.is_empty() {
        return Ok(Observation::Absent);
    }
    if rows.len() > 50 {
        return Ok(Observation::Unqualified("observation_limit"));
    }
    let mut raw_total = 0_i64;
    for row in &rows {
        if !capture_qualified(row, stream)
            || row.get::<String, _>("stream") != stream.as_str()
            || row.get::<String, _>("record_representation") != stream.representation()
            || row.get::<i64, _>("capture_sequence") != row.get::<i64, _>("sequence")
            || row.get::<i64, _>("sequence") > final_sequence
        {
            return Ok(Observation::Unqualified("capture_unqualified"));
        }
        raw_total = raw_total.saturating_add(row.get("raw_byte_len"));
        if raw_total > CAPTURE_LIMIT {
            return Ok(Observation::Unqualified("raw_unit_limit"));
        }
    }
    let mut semantic: Option<Vec<u8>> = None;
    let mut selected: Option<RetainedRecord> = None;
    for row in rows {
        let capture_id = row.get("capture_id");
        let raw = open_capture(conn, key, org, snapshot_id, capture_id, &row).await?;
        let Ok(records) = import_source::extract_page(stream, &raw) else {
            return Ok(Observation::Unqualified("malformed_retained_page"));
        };
        let Some(record) = usize::try_from(row.get::<i32, _>("ordinal"))
            .ok()
            .and_then(|n| records.get(n))
            .cloned()
        else {
            return Ok(Observation::Unqualified("ordinal_mismatch"));
        };
        let observed = crypto::snapshot_hmac(
            key,
            org,
            &format!("semantic:{}", stream.representation()),
            &record.canonical,
        );
        if record.source_id.as_deref() != Some(source_id)
            || row.get::<Vec<u8>, _>("semantic_hmac") != observed
        {
            return Ok(Observation::Unqualified("index_integrity"));
        }
        if semantic
            .as_ref()
            .is_some_and(|previous| previous.as_slice() != observed)
        {
            return Ok(Observation::Unqualified("conflicting_observations"));
        }
        semantic = Some(observed.to_vec());
        if selected.is_none() {
            selected = Some(RetainedRecord {
                record,
                capture_id,
                ordinal: row.get("ordinal"),
                raw_bytes: raw_total,
            });
        }
    }
    Ok(Observation::Qualified(Box::new(
        selected.ok_or(MigrationError::SourceNotEligible)?,
    )))
}

/// Requires completed original-stream qualification before candidate iteration.
/// Presence, even a Trash/held Person, excludes original records from admission.
pub async fn original_contains(
    conn: &mut PgConnection,
    org: OrganizationId,
    snapshot_id: Uuid,
    final_sequence: i64,
    source_id: &str,
) -> Result<bool, MigrationError> {
    if !valid_source_id(source_id) {
        return Err(MigrationError::SourceNotEligible);
    }
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family='people' AND source_id=$3 AND capture_sequence<=$4)")
        .bind(snapshot_id).bind(org.0).bind(source_id).bind(final_sequence).fetch_one(conn).await?)
}

/// Compatibility adapter for the in-flight worker while it switches to explicit
/// Observation handling. It fails closed on ambiguity instead of inventing absence.
pub async fn retained_person(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    snapshot_id: Uuid,
    source_id: &str,
) -> Result<Option<(ExtractedRecord, Uuid, i32)>, MigrationError> {
    let sequence = sqlx::query_scalar(
        "SELECT capture_sequence FROM migration_snapshot WHERE id=$1 AND organization_id=$2",
    )
    .bind(snapshot_id)
    .bind(org.0)
    .fetch_one(&mut *conn)
    .await?;
    match retained_record(
        conn,
        key,
        org,
        snapshot_id,
        Stream::People,
        sequence,
        source_id,
    )
    .await?
    {
        Observation::Absent => Ok(None),
        Observation::Qualified(value) => Ok(Some((value.record, value.capture_id, value.ordinal))),
        Observation::Unqualified(_) => Err(MigrationError::SourceNotEligible),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_positive_source_ids_never_round_or_truncate() {
        for invalid in ["", "0", "01", "-1", "+1", "1.0", "1e3", " 1", "1 ", "١"] {
            assert!(!valid_source_id(invalid));
        }
        assert!(valid_source_id("1"));
        assert!(valid_source_id(&"9".repeat(128)));
        assert!(!valid_source_id(&"9".repeat(129)));
    }
    #[test]
    fn qualification_cursor_round_trips_without_source_content() {
        let value = QualificationCursor {
            sequence: 51,
            accepted_captures: 9,
            records: 801,
        };
        let encoded = serde_json::to_vec(&value).unwrap();
        let decoded: QualificationCursor = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            (decoded.sequence, decoded.accepted_captures, decoded.records),
            (51, 9, 801)
        );
        assert!(serde_json::from_str::<QualificationCursor>(
            r#"{"sequence":1,"accepted_captures":1,"records":1,"source":"untrusted"}"#
        )
        .is_err());
    }
}

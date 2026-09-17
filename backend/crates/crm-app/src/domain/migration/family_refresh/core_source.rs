//! One bounded retained core-capture transaction. Every occurrence is retained
//! before Person filtering; this module does not choose a canonical variant or
//! authorize a native write. Raw captures remain in their original store.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    model::Family,
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{
        activity_source, core_change_store, crypto, metadata_source, metadata_store,
        snapshot::SnapshotPolicy,
        snapshot_source::{self, Cursor, Request, Stream},
        MigrationError,
    },
    ids::OrganizationId,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

const RAW_LIMIT: usize = 4 * 1024 * 1024;
const PAGE_RESERVATION: i64 = 64 * 1024 * 1024;
pub const CAPTURE_SQL:&str="SELECT * FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4 AND stream IN ('people','custom_fields','users','notes','note_detail','tasks_open','tasks_completed') ORDER BY sequence LIMIT 1";
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Indexed {
        observations: usize,
        qualified: usize,
    },
    Finished,
    Capacity,
}
#[derive(Serialize, Deserialize)]
struct PageEvidence {
    stream: Stream,
    cursor: Cursor,
    source_id: Option<String>,
    next: Option<Cursor>,
    classification: String,
}
/// No Debug: derived records contain customer content. IDs are authenticated
/// references to original evidence, not new trusted business identifiers.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "record", rename_all = "snake_case")]
pub(super) enum Derived {
    Metadata(metadata_source::Record),
    Activity(activity_source::Record),
    Oversized { record_id: Option<Uuid> },
}
struct Observation {
    source: Option<String>,
    person: Option<String>,
    semantic: [u8; 32],
    derived: Derived,
}
fn observations(
    stream: Stream,
    raw: &[u8],
    key: &RawPayloadKey,
    org: OrganizationId,
) -> Result<Vec<Observation>, MigrationError> {
    let purpose = format!("semantic:{}", stream.representation());
    if matches!(stream, Stream::People | Stream::CustomFields) {
        metadata_source::extract_page(stream, raw)
            .map_err(|_| MigrationError::SourceNotEligible)
            .map(|rows| {
                rows.into_iter()
                    .map(|mut r| {
                        let semantic = crypto::snapshot_hmac(key, org, &purpose, &r.canonical);
                        r.canonical.clear();
                        Observation {
                            source: r.source_id.clone(),
                            person: if stream == Stream::People {
                                r.source_id.clone()
                            } else {
                                None
                            },
                            semantic,
                            derived: Derived::Metadata(r),
                        }
                    })
                    .collect()
            })
    } else {
        activity_source::extract_page(stream, raw)
            .map_err(|_| MigrationError::SourceNotEligible)
            .map(|rows| {
                rows.into_iter()
                    .map(|mut r| {
                        let semantic = crypto::snapshot_hmac(key, org, &purpose, &r.canonical);
                        r.canonical.clear();
                        Observation {
                            source: r.source_id.clone(),
                            person: r.person_id.clone(),
                            semantic,
                            derived: Derived::Activity(r),
                        }
                    })
                    .collect()
            })
    }
}
fn kind(stream: Stream) -> &'static str {
    match stream {
        Stream::People => "person",
        Stream::CustomFields => "field",
        Stream::Users => "user",
        Stream::Notes | Stream::NoteDetail => "note",
        Stream::TasksOpen | Stream::TasksCompleted => "task",
        Stream::Stages => unreachable!("capture query restricts streams"),
    }
}
fn fingerprint(
    key: &RawPayloadKey,
    org: OrganizationId,
    request: &Request,
) -> Result<[u8; 32], MigrationError> {
    request
        .path()
        .map_err(|_| MigrationError::SourceNotEligible)?;
    Ok(crypto::snapshot_hmac(key,org,"request",&serde_json::to_vec(&json!({"stream":request.stream.as_str(),"offset":request.cursor.offset,"next":request.cursor.next,"source_id":request.source_id})).map_err(|_|MigrationError::Crypto)?))
}
async fn request(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    report: Uuid,
    c: &PgRow,
    stream: Stream,
) -> Result<Request, MigrationError> {
    let checkpoint: i64 = c.get("checkpoint");
    let mut cursor = Cursor::default();
    let source_id = if stream == Stream::NoteDetail {
        Some(sqlx::query_scalar::<_,String>("SELECT source_id FROM migration_core_change_note_key WHERE report_id=$1 AND organization_id=$2 AND side=1 AND request_hmac=$3")
            .bind(report).bind(scope.organization.0).bind(c.get::<Vec<u8>,_>("request_fingerprint")).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?)
    } else {
        if checkpoint > 0 {
            let previous=sqlx::query("SELECT id,nonce,ciphertext FROM migration_family_refresh_core_page WHERE bundle_id=$1 AND organization_id=$2 AND plan_id=$3 AND stream=$4 AND checkpoint=$5 AND accepted AND capture_sequence<$6")
                .bind(scope.bundle).bind(scope.organization.0).bind(scope.plan).bind(stream.as_str()).bind(checkpoint-1).bind(c.get::<i64,_>("sequence")).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?;
            let previous: PageEvidence = scope.open(
                key,
                previous.get("id"),
                Purpose::Binding,
                previous.get("nonce"),
                previous.get("ciphertext"),
            )?;
            if previous.stream != stream {
                return Err(MigrationError::Crypto);
            }
            cursor = previous.next.ok_or(MigrationError::SourceNotEligible)?;
        } else if checkpoint < 0 {
            return Err(MigrationError::SourceNotEligible);
        }
        None
    };
    let request = Request {
        stream,
        cursor,
        source_id,
    };
    if fingerprint(key, scope.organization, &request)?.as_slice()
        != c.get::<Vec<u8>, _>("request_fingerprint")
    {
        return Err(MigrationError::Crypto);
    }
    Ok(request)
}
/// Index one capture (at most 100 observations) and charge its derived evidence
/// atomically. The frozen completed report supplies the upper sequence boundary.
pub async fn index_page(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    policy: &SnapshotPolicy,
) -> Result<Progress, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if b.get::<Option<Uuid>, _>("payer_plan_id") != Some(claim.plan) {
        return Err(MigrationError::Conflict);
    }
    let phase: String = p.get("phase");
    if phase != "capture" {
        return if matches!(
            phase.as_str(),
            "mappings" | "classify" | "apply" | "finished"
        ) {
            Ok(Progress::Finished)
        } else {
            Err(MigrationError::Conflict)
        };
    }
    let family = match p.get::<String, _>("family").as_str() {
        "metadata" => Family::Metadata,
        "activity" => Family::Activity,
        _ => return Err(MigrationError::InvalidInput),
    };
    let snapshot: Uuid = p
        .get::<Option<Uuid>, _>("source_snapshot_id")
        .ok_or(MigrationError::SourceNotEligible)?;
    let report_id: Uuid = b
        .get::<Option<Uuid>, _>("core_report_id")
        .ok_or(MigrationError::SourceNotEligible)?;
    let report=sqlx::query("SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2 AND state='completed'")
        .bind(report_id).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::SourceNotEligible)?;
    if report.get::<Uuid, _>("newer_snapshot_id") != snapshot
        || report.get::<Uuid, _>("parent_import_id") != b.get::<Uuid, _>("parent_import_id")
        || report.get::<Uuid, _>("parent_plan_id") != b.get::<Uuid, _>("parent_plan_id")
        || report.get::<i64, _>("source_account_id") != b.get::<i64, _>("source_account_id")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let inputs = core_change_store::validate(&mut tx, key, claim.organization, &report).await?;
    if inputs["source_scope"] != "consistent_identity" {
        return Err(MigrationError::SourceNotEligible);
    }
    let boundary: i64 = report.get("newer_sequence");
    let c = sqlx::query(CAPTURE_SQL)
        .bind(snapshot)
        .bind(claim.organization.0)
        .bind(p.get::<i64, _>("capture_checkpoint"))
        .bind(boundary)
        .fetch_optional(&mut *tx)
        .await?;
    // Final phase transition also changes measured bytes and needs settlement.
    let amount = if c.is_some() { PAGE_RESERVATION } else { 8192 };
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, amount).await? else {
        return Ok(Progress::Capacity);
    };
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family,
        revision: p.get("revision"),
    };
    let (progress, checkpoint, next_phase) = if let Some(c) = c {
        let sequence: i64 = c.get("sequence");
        let progress = index_capture(
            &mut tx,
            key,
            scope,
            snapshot,
            report_id,
            b.get("source_account_id"),
            &c,
        )
        .await?;
        (progress, sequence, "capture")
    } else {
        (Progress::Finished, p.get("capture_checkpoint"), "mappings")
    };
    let n=sqlx::query("UPDATE migration_family_refresh_plan SET capture_checkpoint=$4,phase=$5 WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND phase='capture' AND state='preparing'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(checkpoint).bind(next_phase).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    if n != 1 {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(reservation)
        .bind(claim.epoch)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,bundle_id=%claim.bundle,plan_id=%claim.plan,capture_sequence=checkpoint,"Family refresh core evidence indexed");
    Ok(progress)
}
async fn index_capture(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    snapshot: Uuid,
    report: Uuid,
    account: i64,
    c: &PgRow,
) -> Result<Progress, MigrationError> {
    let stream =
        Stream::parse(&c.get::<String, _>("stream")).ok_or(MigrationError::SourceNotEligible)?;
    let len: i64 = c.get("raw_byte_len");
    let ciphertext: Vec<u8> = c.get("ciphertext");
    if !(0..=RAW_LIMIT as i64).contains(&len)
        || ciphertext.len() > RAW_LIMIT + 16
        || c.get::<String, _>("representation") != stream.representation()
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let raw = crypto::open_snapshot(
        key,
        scope.organization,
        snapshot,
        c.get("id"),
        "capture",
        c.get("nonce"),
        &ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    if raw.len() as i64 != len {
        return Err(MigrationError::Crypto);
    }
    let request = request(conn, key, scope, report, c, stream).await?;
    let linked=sqlx::query("SELECT id,ordinal,source_id,family,representation,semantic_hmac,capture_sequence FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 ORDER BY ordinal LIMIT 101")
        .bind(c.get::<Uuid,_>("id")).bind(snapshot).bind(scope.organization.0).fetch_all(&mut *conn).await?;
    if linked.len() > 100 {
        return Err(MigrationError::SourceNotEligible);
    }
    let accepted: bool = c.get("accepted");
    let truncated: bool = c.get("truncated");
    let status: i32 = c.get("http_status");
    let classification: String = c.get("classification");
    let negative =
        stream == Stream::NoteDetail && status == 404 && classification == "content_inaccessible";
    let mut next = None;
    let mut rows = Vec::new();
    let mut reason = None;
    if negative {
        if !accepted || truncated || !linked.is_empty() {
            return Err(MigrationError::SourceNotEligible);
        }
        reason = Some("source_unavailable");
    } else if truncated || !(200..300).contains(&status) {
        if accepted || !linked.is_empty() {
            return Err(MigrationError::SourceNotEligible);
        }
        reason = Some("capture_not_accepted");
    } else {
        let parsed = observations(stream, &raw, key, scope.organization);
        if accepted || !linked.is_empty() {
            rows = parsed?;
            let qualified = snapshot_source::parse(&request, &raw)
                .map_err(|_| MigrationError::SourceNotEligible)?;
            if rows.len() != linked.len()
                || qualified.records.len() != rows.len()
                || (accepted
                    && (classification != "success" || rows.iter().any(|r| r.source.is_none())))
            {
                return Err(MigrationError::SourceNotEligible);
            }
            for (ordinal, (r, stored)) in rows.iter().zip(&linked).enumerate() {
                let semantic = crypto::snapshot_hmac(
                    key,
                    scope.organization,
                    &format!("semantic:{}", stream.representation()),
                    &qualified.records[ordinal].canonical,
                );
                if stored.get::<i32, _>("ordinal") != ordinal as i32
                    || stored.get::<Option<String>, _>("source_id") != r.source
                    || stored.get::<String, _>("family") != stream.family().as_str()
                    || stored.get::<String, _>("representation") != stream.representation()
                    || stored.get::<Vec<u8>, _>("semantic_hmac") != r.semantic
                    || semantic != r.semantic
                    || stored.get::<i64, _>("capture_sequence") != c.get::<i64, _>("sequence")
                {
                    return Err(MigrationError::Crypto);
                }
            }
            if accepted {
                next = qualified.next;
            }
        } else if let Ok(parsed) = parsed {
            rows = parsed;
        }
        if !accepted {
            reason = Some("capture_not_accepted");
        }
    }
    let page = Uuid::new_v4();
    let sealed = scope.seal(
        key,
        page,
        Purpose::Binding,
        &PageEvidence {
            stream: request.stream,
            cursor: request.cursor,
            source_id: request.source_id,
            next,
            classification,
        },
    )?;
    sqlx::query("INSERT INTO migration_family_refresh_core_page(id,bundle_id,plan_id,organization_id,snapshot_id,capture_id,capture_sequence,checkpoint,stream,accepted,reason,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(page).bind(scope.bundle).bind(scope.plan).bind(scope.organization.0).bind(snapshot).bind(c.get::<Uuid,_>("id")).bind(c.get::<i64,_>("sequence")).bind(c.get::<i64,_>("checkpoint")).bind(stream.as_str()).bind(accepted).bind(reason).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    let total = rows.len();
    let mut qualified = 0;
    for (ordinal, row) in rows.into_iter().enumerate() {
        let id = Uuid::new_v4();
        let mut why = reason.or(if row.source.is_none() {
            Some("invalid_source_id")
        } else {
            None
        });
        let sealed = match scope.seal(key, id, Purpose::Source, &row.derived) {
            Ok(v) => v,
            Err(MigrationError::StorageLimit) => {
                why = Some("unit_too_large");
                scope.seal(
                    key,
                    id,
                    Purpose::Source,
                    &Derived::Oversized {
                        record_id: linked.get(ordinal).map(|r| r.get("id")),
                    },
                )?
            }
            Err(e) => return Err(e),
        };
        let field_name_key = match &row.derived {
            Derived::Metadata(record) => match &record.entity {
                metadata_source::Entity::Field(field) => field.name.as_ref().map(|name| {
                    metadata_store::source_key(
                        key,
                        scope.organization,
                        account,
                        "field-name",
                        name.as_bytes(),
                    )
                }),
                _ => None,
            },
            _ => None,
        };
        sqlx::query("INSERT INTO migration_family_refresh_source(id,bundle_id,plan_id,organization_id,core_page_id,capture_id,capture_sequence,ordinal,representation,kind,source_id,source_person_id,semantic_hmac,qualified,reason,nonce,ciphertext,field_name_hmac,catalog_names_indexed) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)")
            .bind(id).bind(scope.bundle).bind(scope.plan).bind(scope.organization.0).bind(page).bind(c.get::<Uuid,_>("id")).bind(c.get::<i64,_>("sequence")).bind(ordinal as i32).bind(stream.representation()).bind(kind(stream)).bind(row.source).bind(row.person).bind(row.semantic.as_slice()).bind(why.is_none()).bind(why).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(field_name_key).bind(stream==Stream::CustomFields).execute(&mut *conn).await?;
        qualified += usize::from(why.is_none());
    }
    Ok(Progress::Indexed {
        observations: total,
        qualified,
    })
}

/// Require exhausted streams for the selected family, independently of the
/// report's terminal label. The index already authenticated each cursor chain;
/// here its last accepted page must actually close that chain.
pub(super) async fn qualify_family(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    snapshot: Uuid,
    family: Family,
) -> Result<Result<(), super::model::Hold>, MigrationError> {
    use super::model::Hold;
    let streams: &[&str] = match family {
        Family::Metadata => &["people", "custom_fields"],
        Family::Activity => &[
            "people",
            "users",
            "notes",
            "note_detail",
            "tasks_open",
            "tasks_completed",
        ],
        Family::History => return Err(MigrationError::InvalidInput),
    };
    let rows=sqlx::query("SELECT s.stream,s.state,p.id,p.nonce,p.ciphertext FROM migration_snapshot_stream s LEFT JOIN LATERAL (SELECT id,nonce,ciphertext FROM migration_family_refresh_core_page p WHERE p.bundle_id=$1 AND p.organization_id=s.organization_id AND p.plan_id=$2 AND p.snapshot_id=s.snapshot_id AND p.stream=s.stream AND p.accepted ORDER BY p.checkpoint DESC LIMIT 1) p ON true WHERE s.snapshot_id=$3 AND s.organization_id=$4 AND s.stream=ANY($5)")
        .bind(scope.bundle).bind(scope.plan).bind(snapshot).bind(scope.organization.0).bind(streams).fetch_all(&mut *conn).await?;
    if rows.len() != streams.len() {
        return Ok(Err(Hold::SourceUnavailable));
    }
    for row in rows {
        if row.get::<String, _>("state") != "completed" {
            return Ok(Err(Hold::SourceUnavailable));
        }
        let stream = row.get::<String, _>("stream");
        if stream == "note_detail" {
            let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_snapshot_note_detail WHERE snapshot_id=$1 AND organization_id=$2 AND NOT settled)").bind(snapshot).bind(scope.organization.0).fetch_one(&mut *conn).await?;
            if pending {
                return Ok(Err(Hold::SourceUnavailable));
            }
            continue;
        }
        let Some(id) = row.get::<Option<Uuid>, _>("id") else {
            return Ok(Err(Hold::SourceUnavailable));
        };
        let page: PageEvidence = scope.open(
            key,
            id,
            Purpose::Binding,
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        if page.stream.as_str() != stream {
            return Err(MigrationError::Crypto);
        }
        if page.next.is_some() || page.classification != "success" {
            return Ok(Err(Hold::SourceUnavailable));
        }
    }
    Ok(Ok(()))
}

//! Durable, fenced source claims. A database connection is never retained across FUB I/O.
use super::{
    crypto,
    reader::{self, Capture, FubReader, ReaderError},
    snapshot::{self, SnapshotPolicy},
    snapshot_source::{self, Cursor, Request, Stream},
    store, MigrationError,
};
use crate::{
    config::RawPayloadKey,
    ids::{OrganizationId, UserId},
};
use sqlx::{PgPool, Row};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;
static SESSION: OnceLock<Uuid> = OnceLock::new();
pub fn spawn(
    pool: PgPool,
    key: RawPayloadKey,
    reader: Arc<dyn FubReader>,
    policy: SnapshotPolicy,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            // Interleave bounded source and DB-only work so an active download
            // cannot starve retained previews. Both keep their own durable leases.
            for _ in 0..32 {
                let source = match run_once(&pool, &key, reader.as_ref(), &policy).await {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(outcome=%error,"snapshot sweep failed");
                        false
                    }
                };
                let preview = match super::snapshot_preview::run_once(&pool, &key, &policy).await {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(outcome=%error,"snapshot preview sweep failed");
                        false
                    }
                };
                if !source && !preview {
                    break;
                }
            }
        }
    })
}
pub struct Claim {
    pub id: Uuid,
    pub org: OrganizationId,
    pub token: Uuid,
    pub actor: UserId,
    pub connection: Uuid,
    pub revision: i32,
    pub account: i64,
    pub stream: String,
    pub checkpoint: i64,
    pub attempts: i32,
    pub request: Option<Request>,
    credential: String,
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    policy: &SnapshotPolicy,
) -> Result<bool, MigrationError> {
    let _permit = reader::source_read_permit().await;
    let Some(c) = claim(pool, key, policy).await? else {
        return Ok(false);
    };
    let outcome = if let Some(request) = &c.request {
        reader.snapshot(&c.credential, request).await
    } else {
        reader
            .identity(&c.credential)
            .await
            .and_then(|(identity, body)| {
                let capture = Capture {
                    status: 200,
                    body,
                    truncated: false,
                    source_version: None,
                };
                if identity.account_id != c.account || identity.user_id.unwrap_or_default() <= 0 {
                    Err(ReaderError::IdentityMismatch.with_capture(capture))
                } else {
                    Ok(capture)
                }
            })
    };
    settle(pool, key, &c, outcome).await?;
    Ok(true)
}
async fn claim(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
) -> Result<Option<Claim>, MigrationError> {
    let candidate=sqlx::query("SELECT id,organization_id FROM migration_snapshot WHERE ((state IN ('queued','waiting_retry') AND (next_attempt_at IS NULL OR next_attempt_at<=now())) OR (state='running' AND lease_expires_at<=now())) ORDER BY created_at LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let id: Uuid = candidate.get("id");
    let org = OrganizationId::new(candidate.get("organization_id"));
    let mut tx = pool.begin().await?;
    store::lock_org(&mut tx, org).await?;
    let r = snapshot::row(&mut tx, org, id).await?;
    if !matches!(
        r.get::<String, _>("state").as_str(),
        "queued" | "waiting_retry" | "running"
    ) || r
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("lease_expires_at")
        .is_some_and(|v| v > chrono::Utc::now())
    {
        return Ok(None);
    }
    let actor = UserId::new(r.get("initiated_by_user_id"));
    let connection=sqlx::query("SELECT * FROM migration_connection WHERE id=$1 AND organization_id=$2 AND revision=$3 AND status='connected' FOR UPDATE").bind(r.get::<Uuid,_>("connection_id")).bind(org.0).bind(r.get::<i32,_>("connection_revision")).fetch_optional(&mut *tx).await?;
    let admin=sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active' FOR SHARE").bind(org.0).bind(actor.0).fetch_optional(&mut *tx).await?.is_some();
    sqlx::query("UPDATE migration_snapshot SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
    snapshot::release_source(&mut tx, org, id).await?;
    if connection.is_none() || !admin {
        pause(
            &mut tx,
            org,
            id,
            if !admin {
                "initiator_not_authorized"
            } else {
                "connection_changed"
            },
        )
        .await?;
        tx.commit().await?;
        return Ok(None);
    }
    let connection = connection.expect("checked");
    let session = *SESSION.get_or_init(Uuid::new_v4);
    let identity = r.get::<bool, _>("identity_required")
        || r.get::<String, _>("state") == "running"
        || r.get::<Option<Uuid>, _>("source_session") != Some(session);
    if identity {
        sqlx::query("INSERT INTO migration_snapshot_stream(snapshot_id,organization_id,stream,family) VALUES($1,$2,'identity','identity') ON CONFLICT(snapshot_id,organization_id,stream) DO UPDATE SET state='pending'").bind(id).bind(org.0).execute(&mut *tx).await?;
    }
    // Detail work is exhausted only after notes enumeration is complete and every ID is settled.
    sqlx::query("UPDATE migration_snapshot_stream SET state='completed' WHERE snapshot_id=$1 AND organization_id=$2 AND stream='note_detail' AND EXISTS(SELECT 1 FROM migration_snapshot_stream s WHERE s.snapshot_id=$1 AND s.organization_id=$2 AND s.stream='notes' AND s.state='completed') AND NOT EXISTS(SELECT 1 FROM migration_snapshot_note_detail d WHERE d.snapshot_id=$1 AND d.organization_id=$2 AND NOT d.settled)").bind(id).bind(org.0).execute(&mut *tx).await?;
    let s=sqlx::query("SELECT * FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 AND state<>'completed' AND (stream<>'note_detail' OR EXISTS(SELECT 1 FROM migration_snapshot_stream s WHERE s.snapshot_id=$1 AND s.organization_id=$2 AND s.stream='notes' AND s.state='completed')) ORDER BY CASE stream WHEN 'identity' THEN 0 WHEN 'users' THEN 1 WHEN 'stages' THEN 2 WHEN 'custom_fields' THEN 3 WHEN 'people' THEN 4 WHEN 'notes' THEN 5 WHEN 'note_detail' THEN 6 ELSE 7 END,stream LIMIT 1").bind(id).bind(org.0).fetch_optional(&mut *tx).await?;
    let Some(s) = s else {
        sqlx::query("UPDATE migration_snapshot SET state=CASE WHEN EXISTS(SELECT 1 FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 AND content_gaps>0) THEN 'completed_with_gaps' ELSE 'completed' END,completed_at=now(),pause_reason=NULL WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    };
    let stream: String = s.get("stream");
    let attempts = s.get::<i32, _>("cycle_attempts");
    if attempts >= 3 {
        pause(&mut tx, org, id, "retry_exhausted").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let token = Uuid::new_v4();
    if !snapshot::reserve(
        &mut tx,
        policy,
        org,
        id,
        None,
        token,
        snapshot::SOURCE_RESERVATION,
    )
    .await?
    {
        pause(&mut tx, org, id, "storage_budget_exhausted").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let request = if stream == "identity" {
        None
    } else {
        let parsed = Stream::parse(&stream).ok_or(MigrationError::Conflict)?;
        let cursor = match s.get::<Option<Vec<u8>>, _>("cursor_ciphertext") {
            Some(bytes) => serde_json::from_slice(
                &crypto::open_snapshot(
                    key,
                    org,
                    id,
                    id,
                    &format!("stream_cursor:{stream}"),
                    s.get::<Option<Vec<u8>>, _>("cursor_nonce")
                        .as_deref()
                        .ok_or(MigrationError::Crypto)?,
                    &bytes,
                )
                .map_err(|_| MigrationError::Crypto)?,
            )
            .map_err(|_| MigrationError::Crypto)?,
            None => Cursor::default(),
        };
        let source_id = if stream == "note_detail" {
            sqlx::query_scalar::<_,String>("SELECT source_id FROM migration_snapshot_note_detail WHERE snapshot_id=$1 AND organization_id=$2 AND NOT settled ORDER BY ordinal LIMIT 1").bind(id).bind(org.0).fetch_optional(&mut *tx).await?
        } else {
            None
        };
        Some(Request {
            stream: parsed,
            cursor,
            source_id,
        })
    };
    let credential = crypto::open_credential(
        key,
        org,
        r.get("connection_id"),
        r.get("connection_revision"),
        connection.get("credential_nonce"),
        connection.get("credential_ciphertext"),
    )
    .ok()
    .and_then(|v| String::from_utf8(v).ok());
    let Some(credential) = credential else {
        pause(&mut tx, org, id, "credential_decryption_failed").await?;
        snapshot::release(&mut tx, org, id, token, 0).await?;
        tx.commit().await?;
        return Ok(None);
    };
    sqlx::query("UPDATE migration_snapshot SET state='running',pause_reason=NULL,started_at=COALESCE(started_at,now()),lease_token=$3,lease_expires_at=now()+interval '60 seconds',source_session=$4 WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(token).bind(session).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_snapshot_stream SET state='running',attempts=attempts+1,cycle_attempts=cycle_attempts+1 WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(id).bind(org.0).bind(&stream).execute(&mut *tx).await?;
    let result = Claim {
        id,
        org,
        token,
        actor,
        connection: r.get("connection_id"),
        revision: r.get("connection_revision"),
        account: r.get("source_account_id"),
        stream,
        checkpoint: s.get("checkpoint"),
        attempts: attempts + 1,
        request,
        credential,
    };
    tx.commit().await?;
    Ok(Some(result))
}
pub(crate) async fn pause(
    conn: &mut sqlx::PgConnection,
    org: OrganizationId,
    id: Uuid,
    reason: &str,
) -> Result<(), MigrationError> {
    sqlx::query("UPDATE migration_snapshot SET state='paused',pause_reason=$3,identity_required=true,lease_token=NULL,lease_expires_at=NULL,next_attempt_at=NULL,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(org.0).bind(reason).execute(conn).await?;
    Ok(())
}
#[tracing::instrument(skip_all,fields(organization_id=%c.org.0,snapshot_id=%c.id,stream=%c.stream,attempt=c.attempts))]
async fn settle(
    pool: &PgPool,
    key: &RawPayloadKey,
    c: &Claim,
    outcome: Result<Capture, ReaderError>,
) -> Result<(), MigrationError> {
    let (capture, error) = match outcome {
        Ok(v) => (Some(v), None),
        Err(e) => {
            let (e, v) = e.split();
            (v, Some(e))
        }
    };
    let negative = c.stream == "note_detail"
        && capture
            .as_ref()
            .is_some_and(|v| v.status == 404 && !v.truncated);
    let mut reason = if negative {
        Some("content_inaccessible")
    } else {
        error.as_ref().map(ReaderError::code)
    };
    if reason.is_none()
        && capture
            .as_ref()
            .is_some_and(|v| v.status < 200 || v.status >= 300)
    {
        reason = Some("unclassified_endpoint_denial")
    }
    let mut parsed = None;
    if reason.is_none() {
        if let (Some(request), Some(capture)) = (&c.request, &capture) {
            match snapshot_source::parse(request, &capture.body) {
                Ok(p) => parsed = Some(p),
                Err(e) => reason = Some(e.code()),
            }
        }
    }
    if parsed
        .as_ref()
        .is_some_and(|v| v.records.iter().any(|r| r.source_id.is_none()))
    {
        reason = Some("invalid_source_id")
    }
    let mut tx = pool.begin().await?;
    store::lock_org(&mut tx, c.org).await?;
    let r = snapshot::row(&mut tx, c.org, c.id).await?;
    if r.get::<Option<Uuid>, _>("lease_token") != Some(c.token)
        || r.get::<String, _>("state") != "running"
        || r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("lease_expires_at")
            .is_none_or(|v| v <= chrono::Utc::now())
    {
        return Ok(());
    }
    let connection=sqlx::query("SELECT 1 FROM migration_connection WHERE id=$1 AND organization_id=$2 AND revision=$3 AND status='connected' FOR UPDATE").bind(c.connection).bind(c.org.0).bind(c.revision).fetch_optional(&mut *tx).await?.is_some();
    let authorized=sqlx::query("SELECT 1 FROM organization_membership WHERE organization_id=$1 AND user_id=$2 AND role='admin' AND status='active' FOR SHARE").bind(c.org.0).bind(c.actor.0).fetch_optional(&mut *tx).await?.is_some();
    if !connection || !authorized {
        pause(
            &mut tx,
            c.org,
            c.id,
            if !authorized {
                "initiator_not_authorized"
            } else {
                "connection_changed"
            },
        )
        .await?;
        snapshot::release(&mut tx, c.org, c.id, c.token, 0).await?;
        tx.commit().await?;
        return Ok(());
    }
    let fingerprint = crypto::snapshot_hmac(
        key,
        c.org,
        "request",
        &serde_json::to_vec(&json_request(c)).map_err(|_| MigrationError::Crypto)?,
    );
    if let Some(p) = &parsed {
        if p.next.as_ref().is_some_and(|next| {
            c.request.as_ref().is_some_and(|request| {
                next.offset == request.cursor.offset && next.next == request.cursor.next
            })
        }) {
            reason = Some("pagination_loop")
        }
        if !p.records.is_empty() && c.checkpoint > 0 && c.stream != "note_detail" {
            let ids: Vec<String> = p
                .records
                .iter()
                .filter_map(|r| r.source_id.clone())
                .collect();
            let count=sqlx::query_scalar::<_,i64>("SELECT count(DISTINCT source_id) FROM migration_snapshot_record WHERE snapshot_id=$1 AND organization_id=$2 AND family=$3 AND source_id=ANY($4)").bind(c.id).bind(c.org.0).bind(c.request.as_ref().expect("parsed request").stream.family().as_str()).bind(&ids).fetch_one(&mut *tx).await?;
            let distinct = ids.iter().collect::<std::collections::HashSet<_>>().len();
            if count as usize == distinct {
                reason = Some("pagination_no_progress")
            }
        }
        if p.next.is_some() {
            let repeated=sqlx::query("SELECT 1 FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3 AND request_fingerprint=$4 AND accepted").bind(c.id).bind(c.org.0).bind(&c.stream).bind(fingerprint.as_slice()).fetch_optional(&mut *tx).await?.is_some();
            if repeated {
                reason = Some("pagination_loop")
            }
        }
    }
    let success = reason.is_none();
    let accepted = success || negative;
    let seq = r.get::<i64, _>("capture_sequence") + 1;
    let capture_id = Uuid::new_v4();
    let mut actual = 0_i64;
    let mut raw_len = 0_i64;
    let mut gap_count = if negative { 1 } else { 0 };
    if let Some(capture) = capture {
        raw_len = capture.body.len() as i64;
        let sealed = crypto::seal_snapshot(key, c.org, c.id, capture_id, "capture", &capture.body)
            .map_err(|_| MigrationError::Crypto)?;
        let representation = c
            .request
            .as_ref()
            .map(|r| r.stream.representation())
            .unwrap_or("identity-v1");
        actual += sealed.ciphertext.len() as i64
            + 24
            + 32
            + representation.len() as i64
            + capture
                .source_version
                .as_ref()
                .map_or(0, |v| v.len() as i64);
        sqlx::query("INSERT INTO migration_snapshot_capture(id,snapshot_id,organization_id,stream,sequence,checkpoint,request_fingerprint,representation,http_status,raw_byte_len,nonce,ciphertext,source_version,classification,truncated,accepted) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)").bind(capture_id).bind(c.id).bind(c.org.0).bind(&c.stream).bind(seq).bind(c.checkpoint).bind(fingerprint.as_slice()).bind(representation).bind(i32::from(capture.status)).bind(raw_len).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(capture.source_version).bind(reason.unwrap_or("success")).bind(capture.truncated).bind(accepted).execute(&mut *tx).await?;
        if let Some(p) = &parsed {
            for (ordinal, record) in p.records.iter().enumerate() {
                let id = Uuid::new_v4();
                let projection =
                    serde_json::to_vec(&record.projection).map_err(|_| MigrationError::Crypto)?;
                let sealed = crypto::seal_snapshot(key, c.org, c.id, id, "record", &projection)
                    .map_err(|_| MigrationError::Crypto)?;
                let semantic = crypto::snapshot_hmac(
                    key,
                    c.org,
                    &format!("semantic:{representation}"),
                    &record.canonical,
                );
                let family = c
                    .request
                    .as_ref()
                    .expect("parsed request")
                    .stream
                    .family()
                    .as_str();
                actual += 24
                    + sealed.ciphertext.len() as i64
                    + 32
                    + representation.len() as i64
                    + record.source_id.as_ref().map_or(0, |v| v.len() as i64);
                gap_count += i64::from(record.content_gap);
                sqlx::query("INSERT INTO migration_snapshot_record(id,snapshot_id,organization_id,capture_id,capture_sequence,ordinal,family,source_id,representation,semantic_hmac,projection_nonce,projection_ciphertext,content_gap) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(id).bind(c.id).bind(c.org.0).bind(capture_id).bind(seq).bind(ordinal as i32).bind(family).bind(&record.source_id).bind(representation).bind(semantic.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(record.content_gap).execute(&mut *tx).await?;
                if let Some(source_id) = &record.source_id {
                    if c.stream == "notes" {
                        let inserted=sqlx::query("INSERT INTO migration_snapshot_note_detail(snapshot_id,organization_id,source_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(c.id).bind(c.org.0).bind(source_id).execute(&mut *tx).await?.rows_affected();
                        actual += inserted as i64 * source_id.len() as i64;
                    }
                    for (kind, value) in &record.contact_keys {
                        let hash = crypto::snapshot_hmac(
                            key,
                            c.org,
                            &format!("contact:{kind}"),
                            value.as_bytes(),
                        );
                        let inserted=sqlx::query("INSERT INTO migration_snapshot_contact_key(snapshot_id,organization_id,record_id,capture_sequence,source_id,kind,key_hmac) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(c.id).bind(c.org.0).bind(id).bind(seq).bind(source_id).bind(kind).bind(hash.as_slice()).execute(&mut *tx).await?.rows_affected();
                        actual += inserted as i64 * (source_id.len() + kind.len() + 32) as i64;
                    }
                }
            }
        }
    }
    let oldcursor=sqlx::query("SELECT cursor_nonce,cursor_ciphertext FROM migration_snapshot_stream WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(c.id).bind(c.org.0).bind(&c.stream).fetch_one(&mut *tx).await?;
    if success && c.stream != "identity" && c.stream != "note_detail" {
        let next = parsed.as_ref().and_then(|p| p.next.as_ref());
        let sealed = next
            .map(|next| {
                serde_json::to_vec(next)
                    .map_err(|_| MigrationError::Crypto)
                    .and_then(|v| {
                        crypto::seal_snapshot(
                            key,
                            c.org,
                            c.id,
                            c.id,
                            &format!("stream_cursor:{}", c.stream),
                            &v,
                        )
                        .map_err(|_| MigrationError::Crypto)
                    })
            })
            .transpose()?;
        let oldbytes = oldcursor
            .get::<Option<Vec<u8>>, _>("cursor_ciphertext")
            .map_or(0, |v| v.len() + 24);
        actual -= oldbytes as i64;
        actual += sealed.as_ref().map_or(0, |s| s.ciphertext.len() + 24) as i64;
        sqlx::query("UPDATE migration_snapshot_stream SET cursor_nonce=$4,cursor_ciphertext=$5,state=$6 WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(c.id).bind(c.org.0).bind(&c.stream).bind(sealed.as_ref().map(|s|s.nonce.as_slice())).bind(sealed.as_ref().map(|s|s.ciphertext.as_slice())).bind(if next.is_some(){"pending"}else{"completed"}).execute(&mut *tx).await?;
    }
    if accepted {
        if c.stream == "note_detail" {
            let source_id = c
                .request
                .as_ref()
                .and_then(|r| r.source_id.as_deref())
                .ok_or(MigrationError::Conflict)?;
            sqlx::query("UPDATE migration_snapshot_note_detail SET settled=true WHERE snapshot_id=$1 AND organization_id=$2 AND source_id=$3 AND NOT settled").bind(c.id).bind(c.org.0).bind(source_id).execute(&mut *tx).await?;
            sqlx::query("UPDATE migration_snapshot_stream SET state='pending' WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(c.id).bind(c.org.0).bind(&c.stream).execute(&mut *tx).await?;
        }
        if c.stream == "identity" {
            sqlx::query("UPDATE migration_snapshot_stream SET state='completed' WHERE snapshot_id=$1 AND organization_id=$2 AND stream='identity'").bind(c.id).bind(c.org.0).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE migration_snapshot_stream SET checkpoint=checkpoint+1,cycle_attempts=0,returned_items=returned_items+$4,accepted_captures=accepted_captures+$5,content_gaps=content_gaps+$6,error_code=$7,observed_at=now(),reported_total=COALESCE($8,reported_total) WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(c.id).bind(c.org.0).bind(&c.stream).bind(parsed.as_ref().map_or(0,|p|p.records.len()) as i64).bind(i64::from(success)).bind(gap_count).bind(if negative{reason}else{None}).bind(parsed.as_ref().and_then(|p|p.reported_total.as_deref())).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_snapshot SET state='queued',identity_required=false,lease_token=NULL,lease_expires_at=NULL,pause_reason=NULL,next_attempt_at=NULL WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).execute(&mut *tx).await?;
    } else {
        let delay = match error {
            Some(ReaderError::RateLimited(v)) => Some(v.unwrap_or(60)),
            Some(ReaderError::Unavailable) => Some(30),
            _ => None,
        };
        if let Some(delay) = delay.filter(|d| *d <= 86400 && c.attempts < 3) {
            sqlx::query("UPDATE migration_snapshot SET state='waiting_retry',next_attempt_at=now()+($3::bigint*interval '1 second'),lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(delay as i64).execute(&mut *tx).await?;
        } else {
            pause(
                &mut tx,
                c.org,
                c.id,
                if delay.is_some() && c.attempts >= 3 {
                    "retry_exhausted"
                } else {
                    reason.unwrap_or("source_unavailable")
                },
            )
            .await?;
        }
        sqlx::query("UPDATE migration_snapshot_stream SET state='pending',error_code=$4,observed_at=now() WHERE snapshot_id=$1 AND organization_id=$2 AND stream=$3").bind(c.id).bind(c.org.0).bind(&c.stream).bind(reason).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE migration_snapshot SET raw_bytes=raw_bytes+$3,capture_sequence=$4,accepted_captures=accepted_captures+$5,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(c.id).bind(c.org.0).bind(raw_len).bind(seq).bind(i64::from(success)).execute(&mut *tx).await?;
    let valid=sqlx::query("SELECT 1 FROM migration_snapshot_reservation WHERE token=$1 AND snapshot_id=$2 AND organization_id=$3 AND expires_at>now()").bind(c.token).bind(c.id).bind(c.org.0).fetch_optional(&mut *tx).await?.is_some();
    if !valid {
        return Err(MigrationError::Conflict);
    }
    snapshot::release(&mut tx, c.org, c.id, c.token, actual).await?;
    tx.commit().await?;
    tracing::info!(
        outcome = reason.unwrap_or("success"),
        raw_bytes = raw_len,
        retained_bytes = actual,
        "snapshot capture settled"
    );
    Ok(())
}
fn json_request(c: &Claim) -> serde_json::Value {
    match &c.request {
        Some(r) => {
            serde_json::json!({"stream":c.stream,"offset":r.cursor.offset,"next":r.cursor.next,"source_id":r.source_id})
        }
        None => serde_json::json!({"stream":"identity","checkpoint":c.checkpoint}),
    }
}

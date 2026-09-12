//! Fenced retained-source worker. Transactions end before all source I/O.
use super::{
    crypto, history_capture_source as source, history_capture_store as s,
    reader::{self, Capture, FubReader, ReaderError},
    snapshot::SnapshotPolicy,
    store, MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::envelope::{CommandContext, Origin},
    ids::{CorrelationId, OrganizationId, UserId},
};
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
/// One bounded DB-clock startup boundary per worker process. A verification
/// completed after this boundary can be shared by all compatible live workers.
#[derive(Default)]
pub struct WorkerSession {
    identity_after: tokio::sync::OnceCell<DateTime<Utc>>,
    readiness_admitted: std::sync::atomic::AtomicBool,
}
static SESSION: WorkerSession = WorkerSession {
    identity_after: tokio::sync::OnceCell::const_new(),
    readiness_admitted: std::sync::atomic::AtomicBool::new(false),
};
struct Claim {
    run: Uuid,
    org: OrganizationId,
    token: Uuid,
    request: Option<source::Request>,
    checkpoint: i64,
    total: Option<String>,
    attempt: i32,
    credential: String,
    require_readiness: bool,
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
) -> Result<bool, MigrationError> {
    run_with_session(pool, key, reader, policy, release, &SESSION).await
}
#[cfg(feature = "test-support")]
pub async fn run_once_with_session(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    session: &WorkerSession,
) -> Result<bool, MigrationError> {
    run_with_session(pool, key, reader, policy, release, session).await
}
async fn run_with_session(
    pool: &PgPool,
    key: &RawPayloadKey,
    reader: &dyn FubReader,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    session: &WorkerSession,
) -> Result<bool, MigrationError> {
    let _permit = reader::source_read_permit().await;
    let identity_after = *session
        .identity_after
        .get_or_try_init(|| async {
            sqlx::query_scalar::<_, DateTime<Utc>>("SELECT clock_timestamp()")
                .fetch_one(pool)
                .await
        })
        .await?;
    let initial_readiness = !session
        .readiness_admitted
        .load(std::sync::atomic::Ordering::Acquire);
    let Some(c) = claim(
        pool,
        key,
        policy,
        release,
        identity_after,
        initial_readiness,
    )
    .await?
    else {
        return Ok(false);
    };
    session
        .readiness_admitted
        .store(true, std::sync::atomic::Ordering::Release);
    let result = if let Some(request) = &c.request {
        reader.history(&c.credential, request).await
    } else {
        reader
            .identity(&c.credential)
            .await
            .map(|(_, body)| Capture {
                status: 200,
                body,
                truncated: false,
                source_version: None,
            })
    };
    settle(pool, key, policy, release, &c, result).await?;
    Ok(true)
}
async fn authority(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    r: &sqlx::postgres::PgRow,
    release: Option<&ReleaseReadiness>,
    fresh: bool,
) -> Result<(), &'static str> {
    let org = OrganizationId::new(r.get("organization_id"));
    let ctx = CommandContext {
        organization_id: org,
        actor_user_id: UserId::new(r.get("initiated_by_user_id")),
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(Uuid::new_v4()),
    };
    store::require_admin(conn, &ctx)
        .await
        .map_err(|_| "initiator_not_authorized")?;
    s::source_authority(conn, key, &ctx, r)
        .await
        .map_err(|e| match e {
            MigrationError::Crypto => "retained_integrity_failed",
            MigrationError::SourceNotEligible => "parent_changed",
            _ => "connection_changed",
        })?;
    if !s::current_profile(r) {
        return Err("profile_incompatible");
    }
    if fresh {
        release
            .ok_or("release_not_ready")?
            .require_history_capture(conn)
            .await
            .map_err(|_| "release_not_ready")?;
    }
    Ok(())
}
async fn claim(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    identity_after: DateTime<Utc>,
    initial_readiness: bool,
) -> Result<Option<Claim>, MigrationError> {
    let candidate=sqlx::query("SELECT id,organization_id FROM migration_history_capture_run WHERE (state IN ('queued','waiting_retry') AND (next_attempt_at IS NULL OR next_attempt_at<=now())) OR (state='running' AND lease_expires_at<=now()) ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let run = candidate.get("id");
    let org = OrganizationId::new(candidate.get("organization_id"));
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    store::lock_org(&mut tx, org).await?;
    let r = s::row(&mut tx, org, run).await?;
    if !matches!(
        r.get::<String, _>("state").as_str(),
        "queued" | "waiting_retry" | "running"
    ) || r
        .get::<Option<DateTime<Utc>>, _>("next_attempt_at")
        .is_some_and(|v| v > Utc::now())
        || r.get::<Option<DateTime<Utc>>, _>("lease_expires_at")
            .is_some_and(|v| v > Utc::now())
    {
        return Ok(None);
    }
    let fresh = r.get::<bool, _>("identity_required")
        || r.get::<String, _>("state") == "running"
        || r.get::<Option<DateTime<Utc>>, _>("identity_verified_at")
            .is_none_or(|verified| verified < identity_after);
    let require_readiness = fresh || initial_readiness;
    if let Err(reason) = authority(&mut tx, key, &r, release, require_readiness).await {
        s::pause(&mut tx, org, run, reason).await?;
        tx.commit().await?;
        return Ok(None);
    }
    s::release_kind(&mut tx, org, run, "source").await?;
    let identity = fresh;
    let stream=sqlx::query("SELECT * FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 AND state<>'enumerated' ORDER BY CASE family WHEN 'events' THEN 0 WHEN 'calls' THEN 1 ELSE 2 END LIMIT 1").bind(run).bind(org.0).fetch_optional(&mut *tx).await?;
    if !identity && stream.is_none() {
        s::release_kind(&mut tx, org, run, "control").await?;
        sqlx::query("UPDATE migration_history_capture_run SET state='completed_with_gaps',completed_at=now(),pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL,revision=revision+1 WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    }
    // Terminal identity uncertainty is immutable retained evidence. Never refetch it.
    if !identity
        && stream
            .as_ref()
            .is_some_and(|v| v.get::<String, _>("state") == "terminal_uncertain")
    {
        s::pause(&mut tx, org, run, "enumeration_identity_uncertain").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let (request, checkpoint, total, attempt) = if identity {
        (None, 0, None, r.get::<i32, _>("identity_attempts") + 1)
    } else {
        let stream = stream.ok_or(MigrationError::Conflict)?;
        let family = stream.get::<String, _>("family");
        let parsed = source::Stream::parse(&family).ok_or(MigrationError::Conflict)?;
        let cursor_result = (|| -> Result<source::Cursor, MigrationError> {
            Ok(
                if let Some(ciphertext) = stream.get::<Option<Vec<u8>>, _>("cursor_ciphertext") {
                    let opened = crypto::open_history(
                        key,
                        org,
                        run,
                        run,
                        &format!("checkpoint:{family}"),
                        stream
                            .get::<Option<Vec<u8>>, _>("cursor_nonce")
                            .as_deref()
                            .ok_or(MigrationError::Crypto)?,
                        &ciphertext,
                    )
                    .map_err(|_| MigrationError::Crypto)?;
                    serde_json::from_slice(&opened).map_err(|_| MigrationError::Crypto)?
                } else {
                    source::Cursor::default()
                },
            )
        })();
        let cursor = match cursor_result {
            Ok(v) => v,
            Err(_) => {
                s::pause(&mut tx, org, run, "retained_integrity_failed").await?;
                tx.commit().await?;
                return Ok(None);
            }
        };
        (
            Some(source::Request {
                stream: parsed,
                cursor,
            }),
            stream.get("checkpoint"),
            stream.get("reported_total"),
            stream.get::<i32, _>("cycle_attempts") + 1,
        )
    };
    if attempt > 3 {
        s::pause(&mut tx, org, run, "retry_exhausted").await?;
        tx.commit().await?;
        return Ok(None);
    }
    let token = match s::reserve(&mut tx, org, run, "source", s::REQUEST_RESERVATION, policy).await
    {
        Ok(v) => v,
        Err(MigrationError::StorageLimit) => {
            s::pause(&mut tx, org, run, "storage_limit").await?;
            tx.commit().await?;
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    let (connection, _) = s::connection(
        &mut tx,
        key,
        org,
        r.get("connection_id"),
        r.get("connection_revision"),
    )
    .await?;
    let credential_result = crypto::open_credential(
        key,
        org,
        r.get("connection_id"),
        r.get("connection_revision"),
        connection.get("credential_nonce"),
        connection.get("credential_ciphertext"),
    )
    .map_err(|_| MigrationError::Crypto)
    .and_then(|bytes| String::from_utf8(bytes).map_err(|_| MigrationError::Crypto));
    let credential = match credential_result {
        Ok(v) => v,
        Err(_) => {
            s::pause(&mut tx, org, run, "retained_integrity_failed").await?;
            tx.commit().await?;
            return Ok(None);
        }
    };
    sqlx::query("UPDATE migration_history_capture_run SET state='running',started_at=COALESCE(started_at,now()),lease_token=$3,lease_expires_at=now()+interval '60 seconds',next_attempt_at=NULL,identity_required=$4,identity_attempts=CASE WHEN $4 THEN identity_attempts+1 ELSE identity_attempts END,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(run).bind(org.0).bind(token).bind(identity).execute(&mut *tx).await?;
    if let Some(request) = &request {
        sqlx::query("UPDATE migration_history_stream SET cycle_attempts=cycle_attempts+1,attempts=attempts+1 WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(run).bind(org.0).bind(request.stream.as_str()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(Claim {
        run,
        org,
        token,
        request,
        checkpoint,
        total,
        attempt,
        credential,
        require_readiness,
    }))
}
async fn seen(
    conn: &mut PgConnection,
    c: &Claim,
    family: &str,
    kind: &str,
    digest: &[u8],
) -> Result<bool, MigrationError> {
    Ok(sqlx::query("SELECT 1 FROM migration_history_seen WHERE run_id=$1 AND organization_id=$2 AND family=$3 AND kind=$4 AND digest=$5").bind(c.run).bind(c.org.0).bind(family).bind(kind).bind(digest).fetch_optional(conn).await?.is_some())
}
async fn add_seen(
    conn: &mut PgConnection,
    c: &Claim,
    family: &str,
    kind: &str,
    digest: &[u8],
) -> Result<i64, MigrationError> {
    sqlx::query("INSERT INTO migration_history_seen(run_id,organization_id,family,kind,digest) VALUES($1,$2,$3,$4,$5)").bind(c.run).bind(c.org.0).bind(family).bind(kind).bind(digest).execute(conn).await?;
    Ok(32)
}
#[tracing::instrument(skip_all,fields(organization_id=%c.org.0,run_id=%c.run))]
async fn settle(
    pool: &PgPool,
    key: &RawPayloadKey,
    _policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    c: &Claim,
    result: Result<Capture, ReaderError>,
) -> Result<(), MigrationError> {
    let (capture, error) = match result {
        Ok(c) => (Some(c), None),
        Err(e) => {
            let (e, c) = e.split();
            (c, Some(e))
        }
    };
    let mut reason = error.as_ref().map(ReaderError::code);
    let mut parsed = None;
    if reason.is_none() {
        if let Some(capture) = &capture {
            if capture.truncated || capture.body.len() > 4 * 1024 * 1024 {
                reason = Some("response_too_large")
            } else if !(200..300).contains(&capture.status) {
                reason = Some("unclassified_endpoint_denial")
            } else if let Some(request) = &c.request {
                match source::parse(request, &capture.body, c.total.as_deref()) {
                    Ok(v) => parsed = Some(v),
                    Err(e) => reason = Some(e.code()),
                }
            }
        } else {
            reason = Some("source_unavailable")
        }
    }
    let mut tx = pool.begin().await?;
    store::lock_org(&mut tx, c.org).await?;
    let r = s::row(&mut tx, c.org, c.run).await?;
    if !s::valid_lease(&r, c.token) {
        return Ok(());
    }
    if let Err(reason) = authority(&mut tx, key, &r, release, c.require_readiness).await {
        s::pause(&mut tx, c.org, c.run, reason).await?;
        tx.commit().await?;
        return Ok(());
    }
    let reservation=sqlx::query("SELECT 1 FROM migration_history_reservation WHERE token=$1 AND run_id=$2 AND organization_id=$3 AND expires_at>now()").bind(c.token).bind(c.run).bind(c.org.0).fetch_optional(&mut *tx).await?.is_some();
    if !reservation {
        return Ok(());
    }
    if c.request.is_none() && reason.is_none() {
        match capture.as_ref().map(|v| source::parse_identity(&v.body)) {
            Some(Ok(identity))
                if identity.account_id == r.get::<i64, _>("source_account_id")
                    && identity.user_id == Some(r.get("source_user_id")) => {}
            Some(Err(error)) => reason = Some(error.code()),
            _ => reason = Some("source_identity_mismatch"),
        }
    }
    let family = c
        .request
        .as_ref()
        .map(|v| v.stream.as_str())
        .unwrap_or("identity");
    let page_digest = parsed.as_ref().map(|p| {
        crypto::history_hmac(
            key,
            c.org,
            c.run,
            &format!("page:{family}"),
            &p.page_digest_bytes,
        )
    });
    let next_digest = parsed
        .as_ref()
        .and_then(|p| p.next.as_ref())
        .and_then(|p| p.next.as_ref())
        .map(|t| crypto::history_hmac(key, c.org, c.run, &format!("token:{family}"), t.as_bytes()));
    if let Some(d) = page_digest {
        if parsed.as_ref().is_some_and(|p| p.records.len() == 100)
            && seen(&mut tx, c, family, "page", &d).await?
        {
            reason = Some("pagination_no_progress")
        }
    }
    if let Some(d) = next_digest {
        if seen(&mut tx, c, family, "token", &d).await? {
            reason = Some("pagination_no_progress")
        }
    }
    if reason.is_some() {
        parsed = None
    }
    let sequence = r.get::<i64, _>("capture_sequence") + 1;
    let mut actual = 0i64;
    let mut raw_len = 0i64;
    if let Some(capture) = capture {
        let capture_id = Uuid::new_v4();
        let raw = &capture.body[..capture.body.len().min(4 * 1024 * 1024)];
        raw_len = raw.len() as i64;
        let sealed = crypto::seal_history(key, c.org, c.run, capture_id, "capture", raw)
            .map_err(|_| MigrationError::Crypto)?;
        let representation = c
            .request
            .as_ref()
            .map(|v| v.stream.representation())
            .unwrap_or("fub-history-v1/identity");
        let version = capture.source_version.as_deref().filter(|v| {
            v.len() <= 64
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        });
        actual += (sealed.ciphertext.len()
            + 24
            + 32
            + representation.len()
            + version.map_or(0, str::len)) as i64;
        sqlx::query("INSERT INTO migration_history_capture(id,run_id,organization_id,family,sequence,checkpoint,classification,http_status,representation,source_version,raw_byte_len,nonce,ciphertext,content_hmac,truncated) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(capture_id).bind(c.run).bind(c.org.0).bind(family).bind(sequence).bind(c.checkpoint).bind(if parsed.is_some(){"advancing"}else if c.request.is_none()&&reason.is_none(){"identity"}else{"diagnostic"}).bind(i32::from(capture.status)).bind(representation).bind(version).bind(raw_len).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(crypto::history_hmac(key,c.org,c.run,"capture",raw).as_slice()).bind(capture.truncated||capture.body.len()>raw.len()).execute(&mut *tx).await?;
        if let Some(p) = parsed {
            for (ordinal, record) in p.records.iter().enumerate() {
                actual +=
                    observation(&mut tx, key, c, &r, capture_id, sequence, ordinal, record).await?;
            }
            if p.records.len() == 100 {
                actual += add_seen(
                    &mut tx,
                    c,
                    family,
                    "page",
                    page_digest.as_ref().ok_or(MigrationError::Crypto)?,
                )
                .await?;
            }
            if let Some(d) = next_digest {
                actual += add_seen(&mut tx, c, family, "token", &d).await?;
            }
            let old=sqlx::query("SELECT cursor_nonce,cursor_ciphertext,reported_total FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(c.run).bind(c.org.0).bind(family).fetch_one(&mut *tx).await?;
            actual -= old
                .get::<Option<Vec<u8>>, _>("cursor_nonce")
                .map_or(0, |v| v.len()) as i64
                + old
                    .get::<Option<Vec<u8>>, _>("cursor_ciphertext")
                    .map_or(0, |v| v.len()) as i64;
            if old.get::<Option<String>, _>("reported_total").is_none() {
                actual += p.reported_total.len() as i64
            }
            let checkpoint = p
                .next
                .as_ref()
                .map(|v| {
                    crypto::seal_history(
                        key,
                        c.org,
                        c.run,
                        c.run,
                        &format!("checkpoint:{family}"),
                        &serde_json::to_vec(v).map_err(|_| MigrationError::Crypto)?,
                    )
                    .map_err(|_| MigrationError::Crypto)
                })
                .transpose()?;
            if let Some(v) = &checkpoint {
                actual += (v.nonce.len() + v.ciphertext.len()) as i64
            }
            sqlx::query("UPDATE migration_history_stream SET checkpoint=checkpoint+1,cycle_attempts=0,reported_total=COALESCE(reported_total,$4),cursor_nonce=$5,cursor_ciphertext=$6 WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(c.run).bind(c.org.0).bind(family).bind(&p.reported_total).bind(checkpoint.as_ref().map(|v|v.nonce.as_slice())).bind(checkpoint.as_ref().map(|v|v.ciphertext.as_slice())).execute(&mut *tx).await?;
            if p.next.is_none() {
                let counts=sqlx::query("SELECT occurrences,valid_occurrences,invalid_occurrences,unique_ids FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(c.run).bind(c.org.0).bind(family).fetch_one(&mut *tx).await?;
                let exact = counts.get::<i64, _>("invalid_occurrences") == 0
                    && counts.get::<i64, _>("valid_occurrences")
                        == counts.get::<i64, _>("unique_ids")
                    && counts.get::<i64, _>("unique_ids").to_string() == p.reported_total;
                sqlx::query("UPDATE migration_history_stream SET state=$4 WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(c.run).bind(c.org.0).bind(family).bind(if exact{"enumerated"}else{"terminal_uncertain"}).execute(&mut *tx).await?;
                if !exact {
                    reason = Some("enumeration_identity_uncertain")
                }
            }
        }
        sqlx::query("UPDATE migration_history_capture_run SET capture_sequence=$3,raw_bytes=raw_bytes+$4 WHERE id=$1 AND organization_id=$2").bind(c.run).bind(c.org.0).bind(sequence).bind(raw_len).execute(&mut *tx).await?;
    }
    s::release(&mut tx, c.org, c.run, c.token, actual).await?;
    let retry = matches!(
        error,
        Some(ReaderError::Unavailable | ReaderError::RateLimited(_))
    ) && c.attempt < 3;
    if let Some(reason) = reason {
        if retry {
            let seconds = match error {
                Some(ReaderError::RateLimited(Some(v))) => v.min(86400),
                _ => 2,
            };
            sqlx::query("UPDATE migration_history_capture_run SET state='waiting_retry',pause_reason=$3,next_attempt_at=now()+($4::text||' seconds')::interval,lease_token=NULL,lease_expires_at=NULL,revision=revision+1 WHERE id=$1 AND organization_id=$2").bind(c.run).bind(c.org.0).bind(reason).bind(seconds.to_string()).execute(&mut *tx).await?;
        } else {
            s::pause(&mut tx, c.org, c.run, reason).await?;
        }
    } else {
        sqlx::query("UPDATE migration_history_capture_run SET state='queued',pause_reason=NULL,identity_required=false,identity_verified_at=CASE WHEN $3 THEN clock_timestamp() ELSE identity_verified_at END,identity_attempts=0,lease_token=NULL,lease_expires_at=NULL,revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(c.run).bind(c.org.0).bind(c.request.is_none()).execute(&mut *tx).await?;
    }
    tracing::info!(
        raw_bytes = raw_len,
        retained_bytes = actual,
        outcome = reason.unwrap_or("accepted"),
        "history capture settled"
    );
    tx.commit().await?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn observation(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    c: &Claim,
    r: &sqlx::postgres::PgRow,
    capture: Uuid,
    sequence: i64,
    ordinal: usize,
    record: &source::SourceRecord,
) -> Result<i64, MigrationError> {
    let family = c
        .request
        .as_ref()
        .ok_or(MigrationError::Conflict)?
        .stream
        .as_str();
    let id = Uuid::new_v4();
    let identity = record.source_id.as_ref().map(|v| {
        crypto::history_hmac(
            key,
            c.org,
            c.run,
            &format!("identity:{family}"),
            v.as_bytes(),
        )
    });
    let semantic = crypto::history_hmac(
        key,
        c.org,
        c.run,
        &format!("semantic:{family}"),
        &record.canonical,
    );
    let primary = record
        .primary_person_id
        .as_ref()
        .map(|v| crypto::history_hmac(key, c.org, c.run, "person-reference", v.as_bytes()));
    let (disposition, person) =
        s::parent_link(conn, c.org, r, record.primary_person_id.as_deref()).await?;
    let bytes = serde_json::to_vec(&record.projection).map_err(|_| MigrationError::Crypto)?;
    if bytes.len() > 16 * 1024 {
        return Err(MigrationError::StorageLimit);
    }
    let sealed = crypto::seal_history(key, c.org, c.run, id, "projection", &bytes)
        .map_err(|_| MigrationError::Crypto)?;
    let mut actual = (24
        + sealed.ciphertext.len()
        + 32
        + identity.map_or(0, |_| 32)
        + primary.map_or(0, |_| 32)) as i64;
    let mut unique = 0i64;
    let mut equal = 0i64;
    let mut variant = 0i64;
    let mut old_disposition = None;
    if let Some(identity) = &identity {
        let old=sqlx::query("SELECT disposition,primary_person_hmac FROM migration_history_identity WHERE run_id=$1 AND organization_id=$2 AND family=$3 AND identity_hmac=$4").bind(c.run).bind(c.org.0).bind(family).bind(identity.as_slice()).fetch_optional(&mut *conn).await?;
        if let Some(old) = old {
            let exists=sqlx::query("SELECT 1 FROM migration_history_observation WHERE run_id=$1 AND organization_id=$2 AND family=$3 AND identity_hmac=$4 AND semantic_hmac=$5 LIMIT 1").bind(c.run).bind(c.org.0).bind(family).bind(identity.as_slice()).bind(semantic.as_slice()).fetch_optional(&mut *conn).await?.is_some();
            if exists {
                equal = 1
            } else {
                variant = 1
            }
            let old_link = old.get::<String, _>("disposition");
            if old_link != "conflicting_reference"
                && old
                    .get::<Option<Vec<u8>>, _>("primary_person_hmac")
                    .as_deref()
                    != primary.as_ref().map(|v| v.as_slice())
            {
                old_disposition = Some(old_link);
                sqlx::query("UPDATE migration_history_identity SET disposition='conflicting_reference' WHERE run_id=$1 AND organization_id=$2 AND family=$3 AND identity_hmac=$4").bind(c.run).bind(c.org.0).bind(family).bind(identity.as_slice()).execute(&mut *conn).await?;
            }
        } else {
            unique = 1;
            actual += 32 + primary.map_or(0, |_| 32);
            sqlx::query("INSERT INTO migration_history_identity(run_id,organization_id,family,identity_hmac,primary_person_hmac,disposition) VALUES($1,$2,$3,$4,$5,$6)").bind(c.run).bind(c.org.0).bind(family).bind(identity.as_slice()).bind(primary.as_ref().map(|v|v.as_slice())).bind(disposition).execute(&mut *conn).await?;
        }
    }
    sqlx::query("INSERT INTO migration_history_observation(id,run_id,organization_id,capture_id,capture_sequence,ordinal,family,identity_hmac,semantic_hmac,primary_person_hmac,person_id,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(id).bind(c.run).bind(c.org.0).bind(capture).bind(sequence).bind(ordinal as i32).bind(family).bind(identity.as_ref().map(|v|v.as_slice())).bind(semantic.as_slice()).bind(primary.as_ref().map(|v|v.as_slice())).bind(person).bind(disposition).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    for source in &record.person_refs {
        let (_, target) = s::parent_link(conn, c.org, r, Some(source)).await?;
        let h = crypto::history_hmac(key, c.org, c.run, "person-reference", source.as_bytes());
        sqlx::query("INSERT INTO migration_history_person_link(observation_id,run_id,organization_id,source_person_hmac,person_id) VALUES($1,$2,$3,$4,$5)").bind(id).bind(c.run).bind(c.org.0).bind(h.as_slice()).bind(target).execute(&mut *conn).await?;
        actual += 32;
    }
    sqlx::query("UPDATE migration_history_stream SET occurrences=occurrences+1,valid_occurrences=valid_occurrences+$4,invalid_occurrences=invalid_occurrences+$5,unique_ids=unique_ids+$6,equal_repeats=equal_repeats+$7,conflicting_variants=conflicting_variants+$8 WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(c.run).bind(c.org.0).bind(family).bind(i64::from(identity.is_some())).bind(i64::from(identity.is_none())).bind(unique).bind(equal).bind(variant).execute(&mut *conn).await?;
    if unique == 1 {
        change_link(conn, c, family, disposition, 1).await?
    }
    if let Some(old) = old_disposition {
        change_link(conn, c, family, &old, -1).await?;
        change_link(conn, c, family, "conflicting_reference", 1).await?;
    }
    Ok(actual)
}
async fn change_link(
    conn: &mut PgConnection,
    c: &Claim,
    family: &str,
    kind: &str,
    delta: i64,
) -> Result<(), MigrationError> {
    let column = match kind {
        "linked" => "linked",
        "parent_excluded" => "parent_excluded",
        "no_parent_identity" => "no_parent_identity",
        "invalid_person_reference" => "invalid_person_reference",
        "conflicting_reference" => "conflicting_reference",
        _ => return Err(MigrationError::Conflict),
    };
    sqlx::query(&format!("UPDATE migration_history_stream SET {column}={column}+$4 WHERE run_id=$1 AND organization_id=$2 AND family=$3")).bind(c.run).bind(c.org.0).bind(family).bind(delta).execute(conn).await?;
    Ok(())
}

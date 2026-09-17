//! Bounded retained history walk. Raw bodies stay in their original encrypted
//! capture; this index retains metadata and references, never native facts.
use super::{
    cohort::Claim,
    core_source::Progress,
    evidence::{Purpose, Scope},
    history_source::{self, HistoryEvidence},
    model::{Family, Hold},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{
        crypto,
        history_capture_source::{self as source, Cursor, Request, Stream},
        history_capture_store as capture,
        snapshot::SnapshotPolicy,
        MigrationError,
    },
    ids::OrganizationId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;
const UNIT: i64 = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
struct Page {
    stream: Stream,
    next: Option<Cursor>,
    reported_total: String,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Record {
    pub observation: Option<Uuid>,
    pub evidence: Option<HistoryEvidence>,
    pub reason: Option<Hold>,
    pub person_refs: Vec<String>,
}
fn open_capture(
    key: &RawPayloadKey,
    org: OrganizationId,
    run: Uuid,
    row: &PgRow,
) -> Result<Vec<u8>, MigrationError> {
    let ciphertext: Vec<u8> = row.get("ciphertext");
    if ciphertext.len() > 4 * 1024 * 1024 + 16 || row.get::<Vec<u8>, _>("nonce").len() != 24 {
        return Err(MigrationError::Crypto);
    }
    let bytes = crypto::open_history(
        key,
        org,
        run,
        row.get("id"),
        "capture",
        row.get("nonce"),
        &ciphertext,
    )
    .map_err(|_| MigrationError::Crypto)?;
    if bytes.len() as i64 != row.get::<i64, _>("raw_byte_len")
        || row.get::<Vec<u8>, _>("content_hmac")
            != crypto::history_hmac(key, org, run, "capture", &bytes)
    {
        return Err(MigrationError::Crypto);
    }
    Ok(bytes)
}
async fn binding(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    b: &PgRow,
) -> Result<PgRow, MigrationError> {
    let id: Uuid = b
        .get::<Option<Uuid>, _>("history_capture_id")
        .ok_or(MigrationError::SourceNotEligible)?;
    let r = sqlx::query(
        "SELECT * FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2",
    )
    .bind(id)
    .bind(claim.organization.0)
    .fetch_one(&mut *conn)
    .await?;
    let started = r.get::<Option<DateTime<Utc>>, _>("started_at");
    let completed = r.get::<Option<DateTime<Utc>>, _>("completed_at");
    if !capture::current_profile(&r)
        || r.get::<String, _>("state") != "completed_with_gaps"
        || r.get::<Uuid, _>("parent_import_id") != b.get::<Uuid, _>("parent_import_id")
        || r.get::<Uuid, _>("parent_plan_id") != b.get::<Uuid, _>("parent_plan_id")
        || r.get::<i64, _>("source_account_id") != b.get::<i64, _>("source_account_id")
        || r.get::<Option<i64>, _>("parent_source_user_id") != Some(r.get("source_user_id"))
        || started.is_none()
        || completed.is_none()
        || completed < started
        || completed > Some(b.get("created_at"))
    {
        return Err(MigrationError::SourceNotEligible);
    }
    capture::verify_parent(conn, claim.organization, &r).await?;
    let anchor: Option<DateTime<Utc>> = if let Some(snapshot) =
        b.get::<Option<Uuid>, _>("core_snapshot_id")
    {
        sqlx::query_scalar("SELECT completed_at FROM migration_snapshot WHERE id=$1 AND organization_id=$2 AND state IN ('completed','completed_with_gaps')").bind(snapshot).bind(claim.organization.0).fetch_one(&mut *conn).await?
    } else {
        // A later cohort may lack eligible history without blocking the entire
        // retained index. Each unit separately checks its own creation anchor.
        sqlx::query_scalar("SELECT s.completed_at FROM migration_import i JOIN migration_snapshot s ON s.id=i.snapshot_id AND s.organization_id=i.organization_id WHERE i.id=$1 AND i.organization_id=$2").bind(b.get::<Uuid,_>("parent_import_id")).bind(claim.organization.0).fetch_one(&mut *conn).await?
    };
    if anchor.is_none() || started <= anchor {
        return Err(MigrationError::SourceNotEligible);
    }
    let streams=sqlx::query("SELECT * FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 ORDER BY family").bind(id).bind(claim.organization.0).fetch_all(&mut *conn).await?;
    if streams.len() != 3
        || streams.iter().any(|s| {
            s.get::<String, _>("state") != "enumerated"
                || s.get::<Option<String>, _>("reported_total")
                    != Some(s.get::<i64, _>("unique_ids").to_string())
        })
    {
        return Err(MigrationError::SourceNotEligible);
    }
    let identity=sqlx::query("SELECT * FROM migration_history_capture WHERE run_id=$1 AND organization_id=$2 AND classification='identity' AND sequence<=$3 ORDER BY sequence DESC LIMIT 1").bind(id).bind(claim.organization.0).bind(r.get::<i64,_>("capture_sequence")).fetch_optional(&mut *conn).await?.ok_or(MigrationError::Crypto)?;
    if identity.get::<bool, _>("truncated") || identity.get::<i32, _>("http_status") != 200 {
        return Err(MigrationError::Crypto);
    }
    let identity = source::parse_identity(&open_capture(key, claim.organization, id, &identity)?)
        .map_err(|_| MigrationError::Crypto)?;
    if identity.account_id != r.get::<i64, _>("source_account_id")
        || identity.user_id != Some(r.get("source_user_id"))
    {
        return Err(MigrationError::Crypto);
    }
    Ok(r)
}
pub async fn index_page(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    policy: &SnapshotPolicy,
) -> Result<Progress, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "history" {
        return Err(MigrationError::InvalidInput);
    }
    match p.get::<String, _>("phase").as_str() {
        "capture" => {}
        "mappings" | "classify" | "apply" | "finished" => return Ok(Progress::Finished),
        _ => return Err(MigrationError::Conflict),
    }
    let run = binding(&mut tx, key, claim, &b).await?;
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::History,
        revision: p.get("revision"),
    };
    let raw=sqlx::query("SELECT * FROM migration_history_capture WHERE run_id=$1 AND organization_id=$2 AND sequence>$3 AND sequence<=$4 AND family IN ('events','calls','text_messages') ORDER BY sequence LIMIT 1")
        .bind(run.get::<Uuid,_>("id")).bind(claim.organization.0).bind(p.get::<i64,_>("capture_checkpoint")).bind(run.get::<i64,_>("capture_sequence")).fetch_optional(&mut *tx).await?;
    let Some(reservation) = preparation::reserve(
        &mut tx,
        claim,
        policy,
        if raw.is_some() { UNIT } else { 8192 },
    )
    .await?
    else {
        return Ok(Progress::Capacity);
    };
    let (progress, checkpoint, phase) = if let Some(raw) = raw {
        let progress = index_capture(&mut tx, key, scope, &run, &raw).await?;
        (progress, raw.get::<i64, _>("sequence"), "capture")
    } else {
        for stream in [Stream::Events, Stream::Calls, Stream::TextMessages] {
            let last=sqlx::query("SELECT * FROM migration_family_refresh_history_page WHERE bundle_id=$1 AND organization_id=$2 AND plan_id=$3 AND stream=$4 AND accepted ORDER BY checkpoint DESC LIMIT 1")
                .bind(claim.bundle).bind(claim.organization.0).bind(claim.plan).bind(stream.as_str()).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Crypto)?;
            let page: Page = scope.open(
                key,
                last.get("id"),
                Purpose::Binding,
                last.get("nonce"),
                last.get("ciphertext"),
            )?;
            if page.stream != stream || page.next.is_some() {
                return Err(MigrationError::Crypto);
            }
        }
        (Progress::Finished, p.get("capture_checkpoint"), "mappings")
    };
    let n=sqlx::query("UPDATE migration_family_refresh_plan SET capture_checkpoint=$4,phase=$5 WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND phase='capture' AND state='preparing'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(checkpoint).bind(phase).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
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
    tracing::info!(organization_id=%claim.organization,bundle_id=%claim.bundle,plan_id=%claim.plan,capture_sequence=checkpoint,"Family refresh history evidence indexed");
    Ok(progress)
}
async fn index_capture(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    run: &PgRow,
    raw: &PgRow,
) -> Result<Progress, MigrationError> {
    let stream = Stream::parse(&raw.get::<String, _>("family")).ok_or(MigrationError::Crypto)?;
    let accepted = raw.get::<String, _>("classification") == "advancing";
    let checkpoint: i64 = raw.get("checkpoint");
    let bytes = open_capture(key, scope.organization, run.get("id"), raw)?;
    let previous=sqlx::query("SELECT * FROM migration_family_refresh_history_page WHERE bundle_id=$1 AND organization_id=$2 AND plan_id=$3 AND stream=$4 AND accepted AND checkpoint=$5")
        .bind(scope.bundle).bind(scope.organization.0).bind(scope.plan).bind(stream.as_str()).bind(checkpoint-1).fetch_optional(&mut *conn).await?;
    let cursor = if checkpoint == 0 {
        Cursor::default()
    } else {
        let previous = previous.ok_or(MigrationError::Crypto)?;
        let page: Page = scope.open(
            key,
            previous.get("id"),
            Purpose::Binding,
            previous.get("nonce"),
            previous.get("ciphertext"),
        )?;
        if page.stream != stream {
            return Err(MigrationError::Crypto);
        }
        page.next.ok_or(MigrationError::Crypto)?
    };
    let total:String=sqlx::query_scalar("SELECT reported_total FROM migration_history_stream WHERE run_id=$1 AND organization_id=$2 AND family=$3").bind(run.get::<Uuid,_>("id")).bind(scope.organization.0).bind(stream.as_str()).fetch_one(&mut *conn).await?;
    let parsed = if raw.get::<i32, _>("http_status") == 200
        && !raw.get::<bool, _>("truncated")
        && raw.get::<String, _>("representation") == stream.representation()
    {
        source::parse(&Request { stream, cursor }, &bytes, Some(&total)).ok()
    } else {
        None
    };
    if accepted && parsed.is_none() {
        return Err(MigrationError::Crypto);
    }
    let observations=sqlx::query("SELECT * FROM migration_history_observation WHERE capture_id=$1 AND run_id=$2 AND organization_id=$3 ORDER BY ordinal LIMIT 101").bind(raw.get::<Uuid,_>("id")).bind(run.get::<Uuid,_>("id")).bind(scope.organization.0).fetch_all(&mut *conn).await?;
    if accepted && parsed.as_ref().map(|p| p.records.len()) != Some(observations.len())
        || !accepted && !observations.is_empty()
    {
        return Err(MigrationError::Crypto);
    }
    let page_id = Uuid::new_v4();
    let page = Page {
        stream,
        next: parsed.as_ref().and_then(|p| p.next.clone()),
        reported_total: total,
    };
    let sealed = scope.seal(key, page_id, Purpose::Binding, &page)?;
    sqlx::query("INSERT INTO migration_family_refresh_history_page(id,bundle_id,plan_id,organization_id,run_id,capture_id,capture_sequence,checkpoint,stream,accepted,reason,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(page_id).bind(scope.bundle).bind(scope.plan).bind(scope.organization.0).bind(run.get::<Uuid,_>("id")).bind(raw.get::<Uuid,_>("id")).bind(raw.get::<i64,_>("sequence")).bind(checkpoint).bind(stream.as_str()).bind(accepted).bind(if accepted{None}else{Some("diagnostic_capture")}).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
    let Some(parsed) = parsed else {
        return Ok(Progress::Indexed {
            observations: 0,
            qualified: 0,
        });
    };
    let total = parsed.records.len();
    let mut qualified = 0;
    for (ordinal, record) in parsed.records.into_iter().enumerate() {
        if accepted {
            let old = &observations[ordinal];
            let identity = record.source_id.as_ref().map(|id| {
                crypto::history_hmac(
                    key,
                    scope.organization,
                    run.get("id"),
                    &format!("identity:{}", stream.as_str()),
                    id.as_bytes(),
                )
                .to_vec()
            });
            let person = record.primary_person_id.as_ref().map(|id| {
                crypto::history_hmac(
                    key,
                    scope.organization,
                    run.get("id"),
                    "person-reference",
                    id.as_bytes(),
                )
                .to_vec()
            });
            if old.get::<i32, _>("ordinal") != ordinal as i32
                || old.get::<i64, _>("capture_sequence") != raw.get::<i64, _>("sequence")
                || old.get::<String, _>("family") != stream.as_str()
                || old.get::<Option<Vec<u8>>, _>("identity_hmac") != identity
                || old.get::<Option<Vec<u8>>, _>("primary_person_hmac") != person
                || old.get::<Vec<u8>, _>("semantic_hmac")
                    != crypto::history_hmac(
                        key,
                        scope.organization,
                        run.get("id"),
                        &format!("semantic:{}", stream.as_str()),
                        &record.canonical,
                    )
            {
                return Err(MigrationError::Crypto);
            }
        }
        let account: i64 = run.get("source_account_id");
        let identity = record
            .source_id
            .as_ref()
            .map(|id| {
                serde_json::to_vec(&(account, stream.as_str(), stream.representation(), id)).map(
                    |bytes| {
                        crypto::snapshot_hmac(
                            key,
                            scope.organization,
                            "timeline-import-identity-v1",
                            &bytes,
                        )
                    },
                )
            })
            .transpose()
            .map_err(|_| MigrationError::Crypto)?;
        let semantic = crypto::snapshot_hmac(
            key,
            scope.organization,
            &format!(
                "timeline-import-canonical-v1:{account}:{}:{}",
                stream.as_str(),
                stream.representation()
            ),
            &record.canonical,
        );
        let interpreted = history_source::interpret(
            key,
            scope.organization,
            account,
            run.get("source_user_id"),
            stream,
            &record,
        );
        let (evidence, reason) = match interpreted {
            Ok(e)
                if serde_json::to_vec(e.display.metadata())
                    .map_err(|_| MigrationError::Crypto)?
                    .len()
                    <= 4096 =>
            {
                (
                    Some(e),
                    if accepted {
                        None
                    } else {
                        Some(Hold::SourceUnavailable)
                    },
                )
            }
            Ok(_) => (None, Some(Hold::UnitTooLarge)),
            Err(h) => (None, Some(h)),
        };
        let id = Uuid::new_v4();
        let mut data = Record {
            observation: observations.get(ordinal).map(|r| r.get("id")),
            evidence,
            reason,
            person_refs: record.person_refs,
        };
        let sealed = match scope.seal(key, id, Purpose::Source, &data) {
            Ok(s) => s,
            Err(MigrationError::StorageLimit) => {
                data.person_refs.clear();
                data.evidence = None;
                data.reason = Some(Hold::UnitTooLarge);
                scope.seal(key, id, Purpose::Source, &data)?
            }
            Err(e) => return Err(e),
        };
        let reason = data.reason.map(|h| {
            serde_json::to_value(h)
                .expect("closed hold serializes")
                .as_str()
                .expect("hold is string")
                .to_owned()
        });
        let kind = match stream {
            Stream::Events => "event",
            Stream::Calls => "call",
            Stream::TextMessages => "text",
        };
        sqlx::query("INSERT INTO migration_family_refresh_source(id,bundle_id,plan_id,organization_id,history_page_id,capture_id,capture_sequence,ordinal,representation,kind,source_id,source_person_id,identity_hmac,semantic_hmac,qualified,reason,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)")
            .bind(id).bind(scope.bundle).bind(scope.plan).bind(scope.organization.0).bind(page_id).bind(raw.get::<Uuid,_>("id")).bind(raw.get::<i64,_>("sequence")).bind(ordinal as i32).bind(stream.representation()).bind(kind).bind(record.source_id).bind(record.primary_person_id).bind(identity.as_ref().map(|v|v.as_slice())).bind(semantic.as_slice()).bind(reason.is_none()).bind(reason).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
        qualified += usize::from(data.reason.is_none());
    }
    Ok(Progress::Indexed {
        observations: total,
        qualified,
    })
}

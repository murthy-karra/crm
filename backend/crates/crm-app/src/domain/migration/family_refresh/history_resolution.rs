//! Complete occurrence reconciliation for the metadata-only history index.
//! Selection does not establish first coverage, a native baseline or write rights.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    history_index::Record,
    history_source::HistoryEvidence,
    model::{Family, Hold, Kind},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{crypto, history_capture_source::Stream, MigrationError},
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct Selected {
    pub row: Uuid,
    pub capture: Uuid,
    pub observations: i64,
    pub evidence: HistoryEvidence,
}
pub enum Resolution {
    Ready(Box<Selected>),
    Held(Hold),
}
pub async fn resolve(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    kind: Kind,
    source_id: &str,
) -> Result<Resolution, MigrationError> {
    let (kind, stream) = match kind {
        Kind::Event => ("event", Stream::Events),
        Kind::Call => ("call", Stream::Calls),
        Kind::Text => ("text", Stream::TextMessages),
        _ => return Err(MigrationError::InvalidInput),
    };
    if source_id.is_empty()
        || source_id.len() > 128
        || source_id.starts_with('0')
        || !source_id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(MigrationError::InvalidInput);
    }
    let (mut tx, b, p) = preparation::read_begin(pool, claim).await?;
    if p.get::<String, _>("family") != "history"
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify" | "apply"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let group=sqlx::query("SELECT count(*) AS observations,count(DISTINCT semantic_hmac) AS variants,count(DISTINCT source_person_id)+(COALESCE(bool_or(source_person_id IS NULL),false))::int AS people,bool_and(qualified) AS qualified,bool_or(reason='unit_too_large') AS oversized,count(DISTINCT representation) AS representations,min(representation) AS representation FROM migration_family_refresh_source WHERE bundle_id=$1 AND organization_id=$2 AND kind=$3 AND source_id=$4")
        .bind(claim.bundle).bind(claim.organization.0).bind(kind).bind(source_id).fetch_one(&mut *tx).await?;
    let count: i64 = group.get("observations");
    if count == 0 {
        return Ok(Resolution::Held(Hold::SourceNotObserved));
    }
    if group.get::<i64, _>("variants") != 1
        || group.get::<i64, _>("people") != 1
        || group.get::<i64, _>("representations") != 1
    {
        return Ok(Resolution::Held(Hold::SourceConflict));
    }
    if group.get::<Option<bool>, _>("qualified") != Some(true) {
        return Ok(Resolution::Held(
            if group.get::<Option<bool>, _>("oversized") == Some(true) {
                Hold::UnitTooLarge
            } else {
                Hold::UnsupportedSource
            },
        ));
    }
    if group.get::<Option<String>, _>("representation").as_deref() != Some(stream.representation())
    {
        return Ok(Resolution::Held(Hold::UnsupportedSource));
    }
    let r=sqlx::query("SELECT s.*,p.revision AS source_revision,p.phase AS source_phase,p.family AS owner_family,p.history_capture_id FROM migration_family_refresh_source s JOIN migration_family_refresh_plan p ON p.id=s.plan_id AND p.bundle_id=s.bundle_id AND p.organization_id=s.organization_id WHERE s.bundle_id=$1 AND s.organization_id=$2 AND s.kind=$3 AND s.source_id=$4 ORDER BY s.id LIMIT 1")
        .bind(claim.bundle).bind(claim.organization.0).bind(kind).bind(source_id).fetch_one(&mut *tx).await?;
    if r.get::<String, _>("owner_family") != "history"
        || r.get::<Option<Uuid>, _>("history_capture_id")
            != b.get::<Option<Uuid>, _>("history_capture_id")
        || !matches!(
            r.get::<String, _>("source_phase").as_str(),
            "mappings" | "classify" | "apply" | "finished"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: r.get("plan_id"),
        family: Family::History,
        revision: r.get("source_revision"),
    };
    let data: Record = scope.open(
        key,
        r.get("id"),
        Purpose::Source,
        r.get("nonce"),
        r.get("ciphertext"),
    )?;
    let evidence = data.evidence.ok_or(MigrationError::Crypto)?;
    let identity = crypto::snapshot_hmac(
        key,
        claim.organization,
        "timeline-import-identity-v1",
        &serde_json::to_vec(&(
            b.get::<i64, _>("source_account_id"),
            stream.as_str(),
            stream.representation(),
            source_id,
        ))
        .map_err(|_| MigrationError::Crypto)?,
    );
    if data.reason.is_some()
        || data.person_refs != vec![evidence.source_person.clone()]
        || evidence.identity_hmac != identity
        || r.get::<Option<Vec<u8>>, _>("identity_hmac").as_deref()
            != Some(evidence.identity_hmac.as_slice())
        || r.get::<Vec<u8>, _>("semantic_hmac") != evidence.semantic_hmac
        || r.get::<Option<String>, _>("source_person_id").as_deref()
            != Some(evidence.source_person.as_str())
    {
        return Err(MigrationError::Crypto);
    }
    Ok(Resolution::Ready(Box::new(Selected {
        row: r.get("id"),
        capture: r.get("history_capture_id"),
        observations: count,
        evidence,
    })))
}

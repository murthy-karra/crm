//! Authenticate the original fact owner and the current typed correction. A
//! missing/erased display cannot be recreated from newer source equality.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    history_source::HistoryDisplay,
    model::{Family, Hold, Kind},
    preparation,
    source_policy::{qualify_newer_capture, Boundary},
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{history_import_source, history_import_store, MigrationError},
};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct Baseline {
    pub identity: Uuid,
    pub person: Uuid,
    pub original_fact: Uuid,
    pub capture: Uuid,
    pub semantic: Vec<u8>,
    pub created: Option<DateTime<Utc>>,
    pub version: i64,
    pub version_id: Option<Uuid>,
    pub result: Option<Uuid>,
}
pub enum Discovery {
    Proven(Baseline),
    Held(Hold),
}
/// The identity hash must come from authenticated retained source selection.
pub async fn discover(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    cohort: Uuid,
    kind: Kind,
    identity_hmac: &[u8; 32],
) -> Result<Discovery, MigrationError> {
    let family = match kind {
        Kind::Event => "events",
        Kind::Call => "calls",
        Kind::Text => "text_messages",
        _ => return Err(MigrationError::InvalidInput),
    };
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "history"
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let c=sqlx::query("SELECT c.*,p.id AS live_person FROM migration_family_refresh_cohort c LEFT JOIN person p ON p.id=c.person_id AND p.organization_id=c.organization_id WHERE c.id=$1 AND c.bundle_id=$2 AND c.organization_id=$3")
        .bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if c.get::<Option<Uuid>, _>("live_person").is_none() {
        return Ok(Discovery::Held(Hold::TargetErased));
    }
    let identity=sqlx::query("SELECT * FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2").bind(claim.organization.0).bind(identity_hmac.as_slice()).fetch_optional(&mut *tx).await?;
    let Some(identity) = identity else {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    };
    let person: Uuid = c.get("person_id");
    if identity
        .get::<Option<DateTime<Utc>>, _>("erased_at")
        .is_some()
    {
        return Ok(Discovery::Held(Hold::TargetErased));
    }
    if identity.get::<Option<Uuid>, _>("person_id") != Some(person)
        || identity.get::<String, _>("family") != family
    {
        return Ok(Discovery::Held(Hold::IdentityMismatch));
    }
    let owner = if let Some(root) = identity.get::<Option<Uuid>, _>("admitted_root_id") {
        let Some(admission) = c.get::<Option<Uuid>, _>("admission_id") else {
            return Ok(Discovery::Held(Hold::IdentityMismatch));
        };
        sqlx::query("SELECT history_capture_id AS capture_id,state,COALESCE(completed_at,updated_at) AS terminal_at FROM migration_admitted_history_root WHERE id=$1 AND organization_id=$2 AND parent_import_id=$3 AND parent_plan_id=$4 AND source_account_id=$5 AND admission_id=$6 AND confirmed_plan_id=$7")
            .bind(root).bind(claim.organization.0).bind(b.get::<Uuid,_>("parent_import_id")).bind(b.get::<Uuid,_>("parent_plan_id")).bind(b.get::<i64,_>("source_account_id")).bind(admission).bind(identity.get::<Option<Uuid>,_>("admitted_plan_id")).fetch_optional(&mut *tx).await?
    } else {
        if c.get::<Option<Uuid>, _>("original_result_id").is_none() {
            return Ok(Discovery::Held(Hold::IdentityMismatch));
        }
        sqlx::query("SELECT p.capture_id,r.state,COALESCE(r.completed_at,r.updated_at) AS terminal_at FROM migration_history_import_run r JOIN migration_history_import_plan p ON p.id=r.plan_id AND p.organization_id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2 AND r.parent_import_id=$3 AND p.parent_plan_id=$4 AND p.source_account_id=$5 AND r.confirmed_at IS NOT NULL")
            .bind(identity.get::<Option<Uuid>,_>("owner_run_id")).bind(claim.organization.0).bind(b.get::<Uuid,_>("parent_import_id")).bind(b.get::<Uuid,_>("parent_plan_id")).bind(b.get::<i64,_>("source_account_id")).fetch_optional(&mut *tx).await?
    };
    let Some(owner) = owner else {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    };
    if !matches!(
        owner.get::<String, _>("state").as_str(),
        "completed" | "cancelled"
    ) || owner.get::<DateTime<Utc>, _>("terminal_at") > b.get::<DateTime<Utc>, _>("created_at")
    {
        return Ok(Discovery::Held(Hold::FirstCoverageRequired));
    }
    let fact =
        history_import_store::existing_fact(&mut tx, key, claim.organization, &identity, &identity)
            .await?;
    let Some(fact) = fact else {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    };
    let table = history_import_source::fact_table(family)?;
    let created:Option<DateTime<Utc>>=sqlx::query_scalar(&format!("SELECT source_created_at FROM {table} WHERE id=$1 AND organization_id=$2 AND identity_id=$3"))
        .bind(fact).bind(claim.organization.0).bind(identity.get::<Uuid,_>("id")).fetch_one(&mut *tx).await?;
    let mut baseline = Baseline {
        identity: identity.get("id"),
        person,
        original_fact: fact,
        capture: owner.get("capture_id"),
        semantic: identity.get("semantic_hmac"),
        created,
        version: 1,
        version_id: None,
        result: None,
    };
    if let Some(head)=sqlx::query("SELECT * FROM migration_family_refresh_history_head WHERE organization_id=$1 AND identity_id=$2").bind(claim.organization.0).bind(baseline.identity).fetch_optional(&mut *tx).await? {
        if head.get::<Uuid,_>("original_fact_id")!=fact || head.get::<Uuid,_>("person_id")!=person || head.get::<String,_>("family")!=family{return Ok(Discovery::Held(Hold::IdentityMismatch));}
        if head.get::<i64,_>("version")==1 {
            if head.get::<Uuid,_>("capture_id")!=baseline.capture || head.get::<Vec<u8>,_>("semantic_hmac")!=baseline.semantic || head.get::<Option<DateTime<Utc>>,_>("source_created_at")!=created{return Ok(Discovery::Held(Hold::BaselineUnproven));}
        } else {
            let corrected=table.replace("_imported","_corrected");
            let v=sqlx::query(&format!("SELECT v.*,d.nonce,d.ciphertext,p.revision AS plan_revision FROM {corrected} v JOIN migration_family_refresh_history_display d ON d.id=v.id AND d.identity_id=v.identity_id AND d.organization_id=v.organization_id AND d.result_id=v.result_id JOIN migration_family_refresh_plan p ON p.id=v.plan_id AND p.bundle_id=v.bundle_id AND p.organization_id=v.organization_id JOIN migration_family_refresh_result r ON r.id=v.result_id AND r.manifest_id=v.manifest_id AND r.plan_id=v.plan_id AND r.bundle_id=v.bundle_id AND r.organization_id=v.organization_id AND r.disposition='applied' WHERE v.id=$1 AND v.organization_id=$2 AND v.identity_id=$3"))
                .bind(head.get::<Option<Uuid>,_>("version_id")).bind(claim.organization.0).bind(baseline.identity).fetch_optional(&mut *tx).await?;
            let Some(v)=v else{return Ok(Discovery::Held(Hold::BaselineUnproven));};
            if v.get::<Uuid,_>("original_fact_id")!=fact || v.get::<Uuid,_>("person_id")!=person || v.get::<i64,_>("version")!=head.get::<i64,_>("version") || Some(v.get::<Uuid,_>("result_id"))!=head.get::<Option<Uuid>,_>("result_id") || v.get::<Uuid,_>("capture_id")!=head.get::<Uuid,_>("capture_id") || v.get::<Vec<u8>,_>("semantic_hmac")!=head.get::<Vec<u8>,_>("semantic_hmac") || v.get::<Option<DateTime<Utc>>,_>("source_created_at")!=head.get::<Option<DateTime<Utc>>,_>("source_created_at"){return Ok(Discovery::Held(Hold::BaselineUnproven));}
            let (Some(nonce),Some(ciphertext))=(v.get::<Option<Vec<u8>>,_>("nonce"),v.get::<Option<Vec<u8>>,_>("ciphertext")) else{return Ok(Discovery::Held(Hold::TargetErased));};
            let scope=Scope{organization:claim.organization,bundle:v.get("bundle_id"),plan:v.get("plan_id"),family:Family::History,revision:v.get("plan_revision")};
            let _:HistoryDisplay=scope.open(key,v.get("id"),Purpose::HistoryDisplay,&nonce,&ciphertext)?;
            baseline.capture=v.get("capture_id");baseline.semantic=v.get("semantic_hmac");baseline.created=v.get("source_created_at");baseline.version=v.get("version");baseline.version_id=Some(v.get("id"));baseline.result=Some(v.get("result_id"));
        }
    }
    // The current source was already ordered after the core anchor by indexing.
    // This additionally fences the previous accepted per-identity history state.
    let rows=sqlx::query("SELECT id,source_account_id,started_at,completed_at,state FROM migration_history_capture_run WHERE organization_id=$1 AND parent_import_id=$2 AND parent_plan_id=$3 AND id=ANY($4)")
        .bind(claim.organization.0).bind(b.get::<Uuid,_>("parent_import_id")).bind(b.get::<Uuid,_>("parent_plan_id")).bind(vec![baseline.capture,b.get::<Uuid,_>("history_capture_id")]).fetch_all(&mut *tx).await?;
    let boundary = |id: Uuid| -> Option<Boundary> {
        let r = rows.iter().find(|r| r.get::<Uuid, _>("id") == id)?;
        Some(Boundary {
            capture: id,
            account: r.get("source_account_id"),
            started: r.get::<Option<DateTime<Utc>>, _>("started_at")?,
            completed: r.get("completed_at"),
            terminal: r.get::<String, _>("state") == "completed_with_gaps",
        })
    };
    let (Some(previous), Some(selected)) = (
        boundary(baseline.capture),
        boundary(b.get("history_capture_id")),
    ) else {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    };
    // Core ordering was established by history_index. Separately require a
    // strictly newer history capture than the last applied identity state.
    if let Err(hold) = qualify_newer_capture(selected, previous) {
        return Ok(Discovery::Held(hold));
    }
    Ok(Discovery::Proven(baseline))
}

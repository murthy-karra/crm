//! Read-only eligibility for additions. This is not a baseline and never grants
//! write authority; execution must repeat identity/coverage checks under lock.
use super::{
    cohort::Claim,
    core_resolution, history_resolution,
    model::{Hold, Kind},
    preparation, source_policy,
};
use crate::{config::RawPayloadKey, domain::migration::MigrationError};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct Candidate {
    pub person: Uuid,
    pub source: Uuid,
    pub kind: Kind,
}
pub enum Discovery {
    New(Candidate),
    Held(Hold),
}

/// Resolve all occurrences before checking the frozen Person and the global
/// registry. Equal local content is deliberately irrelevant to new ownership.
pub async fn discover(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    cohort: Uuid,
    kind: Kind,
    source_id: &str,
) -> Result<Discovery, MigrationError> {
    let (source, source_person, history_identity) = match kind {
        Kind::Note | Kind::Task => {
            let source_kind = if kind == Kind::Note {
                core_resolution::Kind::Note
            } else {
                core_resolution::Kind::Task
            };
            let selected =
                match core_resolution::resolve(pool, key, claim, source_kind, source_id).await? {
                    core_resolution::Resolution::Ready(r) => r,
                    core_resolution::Resolution::Held(h) => return Ok(Discovery::Held(h)),
                };
            (selected.row, selected.source_person, None)
        }
        Kind::Event | Kind::Call | Kind::Text => {
            let selected =
                match history_resolution::resolve(pool, key, claim, kind, source_id).await? {
                    history_resolution::Resolution::Ready(r) => r,
                    history_resolution::Resolution::Held(h) => return Ok(Discovery::Held(h)),
                };
            (
                selected.row,
                Some(selected.evidence.source_person),
                Some(selected.evidence.identity_hmac),
            )
        }
        Kind::Metadata => return Err(MigrationError::InvalidInput),
    };
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != kind.family().as_str()
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let c = sqlx::query("SELECT c.*,p.id AS live_person FROM migration_family_refresh_cohort c LEFT JOIN person p ON p.id=c.person_id AND p.organization_id=c.organization_id WHERE c.id=$1 AND c.bundle_id=$2 AND c.organization_id=$3")
        .bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let person: Uuid = c.get("person_id");
    if c.get::<Option<Uuid>, _>("live_person").is_none() {
        return Ok(Discovery::Held(Hold::TargetErased));
    }
    if source_person.as_deref() != Some(c.get::<String, _>("source_person_id").as_str()) {
        return Ok(Discovery::Held(Hold::IdentityMismatch));
    }
    if let Err(h) =
        source_policy::qualify_accepted_scan(&mut tx, claim.organization, &b, &p, &c).await?
    {
        return Ok(Discovery::Held(h));
    }
    let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person p JOIN migration_import_identity i ON i.organization_id=p.organization_id AND i.target_id=p.id AND i.family='people' WHERE p.organization_id=$1 AND p.id=$2 AND i.source_account_id=$3 AND i.source_id=$4 AND i.import_id=$5 AND ((i.admission_id IS NULL AND EXISTS(SELECT 1 FROM migration_import_result r WHERE r.id=$6 AND r.organization_id=i.organization_id AND r.import_id=i.import_id AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=i.target_id AND r.source_id=i.source_id AND r.disposition IN ('imported','already_imported'))) OR i.admission_result_id=$7))")
        .bind(claim.organization.0).bind(person).bind(b.get::<i64,_>("source_account_id")).bind(c.get::<String,_>("source_person_id")).bind(b.get::<Uuid,_>("parent_import_id")).bind(c.get::<Option<Uuid>,_>("original_result_id")).bind(c.get::<Option<Uuid>,_>("admission_result_id")).fetch_one(&mut *tx).await?;
    if !live {
        return Ok(Discovery::Held(Hold::TargetErased));
    }
    let account: i64 = b.get("source_account_id");
    let consumed = if let Some(identity) = history_identity {
        sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2)")
            .bind(claim.organization.0).bind(identity.as_slice()).fetch_one(&mut *tx).await?
    } else {
        let table = if kind == Kind::Note { "note" } else { "task" };
        let native_key = format!("v1:{account}:{source_id}");
        let sql = format!("SELECT EXISTS(SELECT 1 FROM migration_activity_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_id=$4) OR EXISTS(SELECT 1 FROM {table} WHERE organization_id=$1 AND source='fub' AND source_external_id IN ($4,$5))");
        sqlx::query_scalar::<_, bool>(&sql)
            .bind(claim.organization.0)
            .bind(account)
            .bind(table)
            .bind(source_id)
            .bind(native_key)
            .fetch_one(&mut *tx)
            .await?
    };
    if consumed {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    }
    if let Err(h) =
        super::first_coverage::qualify(&mut tx, claim.organization, &b, &p, &c, kind.family())
            .await?
    {
        return Ok(Discovery::Held(h));
    }
    // Mapping, native limits and frozen target allocation follow in classification.
    // A Candidate establishes only authenticated source/identity/coverage scope.
    Ok(Discovery::New(Candidate {
        person,
        source,
        kind,
    }))
}

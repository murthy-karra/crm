//! Read-only first-coverage discovery under the current preparation claim. The
//! immutable result supplies B; present-day rows are only C. Execution must
//! repeat these checks while applying the approved unit.
use super::{
    cohort::Claim,
    metadata_baseline::{self, AfterState, Binding},
    metadata_delta::{Ownership, Snapshot},
    model::Hold,
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{admitted_metadata_worker, metadata_model, metadata_store, MigrationError},
};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct Proven {
    pub result: Uuid,
    pub source_snapshot: Uuid,
    pub capture_sequence: i64,
    pub baseline: Snapshot,
    pub ownership: Ownership,
}
pub enum Discovery {
    Proven(Proven),
    Held(Hold),
}

pub async fn discover(
    pool: &PgPool,
    key: &RawPayloadKey,
    claim: &Claim,
    cohort: Uuid,
) -> Result<Discovery, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "metadata"
        || !matches!(
            p.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let c = sqlx::query("SELECT * FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3")
        .bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let person: Uuid = c.get("person_id");
    let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person p JOIN migration_import_identity i ON i.organization_id=p.organization_id AND i.target_id=p.id AND i.family='people' WHERE p.organization_id=$1 AND p.id=$2 AND i.source_account_id=$3 AND i.source_id=$4 AND i.import_id=$5 AND ((i.admission_id IS NULL AND EXISTS(SELECT 1 FROM migration_import_result r WHERE r.id=$6 AND r.organization_id=i.organization_id AND r.import_id=i.import_id AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=i.target_id AND r.source_id=i.source_id AND r.disposition IN ('imported','already_imported'))) OR i.admission_result_id=$7))")
        .bind(claim.organization.0).bind(person).bind(b.get::<i64,_>("source_account_id")).bind(c.get::<String,_>("source_person_id")).bind(b.get::<Uuid,_>("parent_import_id")).bind(c.get::<Option<Uuid>,_>("original_result_id")).bind(c.get::<Option<Uuid>,_>("admission_result_id")).fetch_one(&mut *tx).await?;
    if !live {
        return Ok(Discovery::Held(Hold::TargetErased));
    }
    match super::native_baseline::load(
        &mut tx,
        key,
        super::native_baseline::Request {
            organization: claim.organization,
            bundle: &b,
            plan: &p,
            cohort: &c,
            kind: super::model::Kind::Metadata,
            target: person,
            source_id: &c.get::<String, _>("source_person_id"),
        },
    )
    .await?
    {
        super::native_baseline::Selection::Absent => {}
        super::native_baseline::Selection::Held(hold) => return Ok(Discovery::Held(hold)),
        super::native_baseline::Selection::Proven(proven) => {
            let mut current =
                metadata_baseline::observe(&mut tx, claim.organization, person).await?;
            current.head = Some(proven.result);
            return Ok(
                match super::native_baseline::verify_metadata(&proven, &current) {
                    Ok((baseline, ownership)) => Discovery::Proven(Proven {
                        result: proven.result,
                        source_snapshot: proven.snapshot,
                        capture_sequence: proven.sequence,
                        baseline,
                        ownership,
                    }),
                    Err(hold) => Discovery::Held(hold),
                },
            );
        }
    }
    let admitted = c.get::<Option<Uuid>, _>("admission_result_id").is_some();
    // All SQL identifiers are closed server-owned alternatives, never input.
    let (prefix, parent_column, source_column, parent_result) = if admitted {
        (
            "migration_admitted_metadata",
            "admission_result_id",
            "source_person_id",
            c.get::<Uuid, _>("admission_result_id"),
        )
    } else {
        (
            "migration_metadata",
            "parent_result_id",
            "source_id",
            c.get::<Uuid, _>("original_result_id"),
        )
    };
    let sql = format!("SELECT r.*,a.snapshot_id,a.capture_sequence,a.state AS owner_state,CASE WHEN a.state='completed' THEN a.completed_at ELSE a.updated_at END AS terminal_at FROM {prefix}_result r JOIN {prefix}_import a ON a.id=r.import_id AND a.organization_id=r.organization_id AND a.confirmed_plan_id=r.plan_id JOIN {prefix}_manifest m ON m.id=r.manifest_id AND m.plan_id=r.plan_id AND m.import_id=r.import_id AND m.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.person_id=$2 AND r.kind='people' AND a.parent_import_id=$3 AND a.parent_plan_id=$4 AND a.source_account_id=$5 AND m.{parent_column}=$6 AND m.{source_column}=$7 AND m.person_id=$2 AND r.committed_at<=$8 ORDER BY r.committed_at DESC,r.id DESC LIMIT 1");
    let cutoff: DateTime<Utc> = b.get("created_at");
    let r = sqlx::query(&sql)
        .bind(claim.organization.0)
        .bind(person)
        .bind(b.get::<Uuid, _>("parent_import_id"))
        .bind(b.get::<Uuid, _>("parent_plan_id"))
        .bind(b.get::<i64, _>("source_account_id"))
        .bind(parent_result)
        .bind(c.get::<String, _>("source_person_id"))
        .bind(cutoff)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(r) = r else {
        return Ok(Discovery::Held(Hold::FirstCoverageRequired));
    };
    if !matches!(
        r.get::<String, _>("owner_state").as_str(),
        "completed" | "cancelled"
    ) || !r
        .get::<Option<DateTime<Utc>>, _>("terminal_at")
        .is_some_and(|at| at <= cutoff)
    {
        return Ok(Discovery::Held(Hold::FirstCoverageRequired));
    }
    if r.get::<String, _>("disposition") == "held" {
        return Ok(Discovery::Held(Hold::BaselineUnproven));
    }
    if let Err(hold) = super::source_policy::qualify_core_snapshots(
        &mut tx,
        claim.organization,
        b.get("source_account_id"),
        p.get::<Option<Uuid>, _>("source_snapshot_id")
            .ok_or(MigrationError::SourceNotEligible)?,
        r.get("snapshot_id"),
    )
    .await?
    {
        return Ok(Discovery::Held(hold));
    }
    let proof = if admitted {
        #[derive(serde::Deserialize)]
        struct Payload {
            #[serde(default)]
            after_state: Option<AfterState>,
        }
        let data: Payload = admitted_metadata_worker::open(
            key,
            claim.organization,
            r.get("snapshot_id"),
            r.get("plan_id"),
            r.get("id"),
            "result",
            &r.get::<Vec<u8>, _>("nonce"),
            &r.get::<Vec<u8>, _>("ciphertext"),
        )?;
        data.after_state
    } else {
        let data: metadata_model::ResultData = metadata_store::open(
            key,
            claim.organization,
            r.get("snapshot_id"),
            r.get("plan_id"),
            r.get("id"),
            "result",
            &r.get::<Vec<u8>, _>("nonce"),
            &r.get::<Vec<u8>, _>("ciphertext"),
        )?;
        data.after_state
    };
    let current = metadata_baseline::observe(&mut tx, claim.organization, person).await?;
    let binding = Binding {
        organization: claim.organization,
        import: r.get("import_id"),
        manifest: r.get("manifest_id"),
        person,
    };
    Ok(
        match metadata_baseline::verify(proof.as_ref(), &binding, Some(&current)) {
            Ok((baseline, ownership)) => Discovery::Proven(Proven {
                result: r.get("id"),
                source_snapshot: r.get("snapshot_id"),
                capture_sequence: r.get("capture_sequence"),
                baseline,
                ownership,
            }),
            Err(hold) => Discovery::Held(hold),
        },
    )
}

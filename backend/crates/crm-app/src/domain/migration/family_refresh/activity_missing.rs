//! A complete source scan does not erase an owned note/task that was absent.
//! Walk exact frozen first owners, reusing observed outcomes and holding absence.
use super::{
    activity_baseline::{self, Discovery},
    activity_walk::Progress,
    cohort::Claim,
    evidence::{Purpose, Scope},
    model::{Baseline, Counts, Family, Hold, Kind},
    preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{activity_store, snapshot::SnapshotPolicy, MigrationError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct MissingProposal {
    pub kind: Kind,
    pub source_id: String,
    pub source_not_observed: bool,
    pub reason: Hold,
    pub baseline: Option<Baseline>,
}
struct Cursor {
    kind: Option<String>,
    source: Option<String>,
}
async fn checkpoint(
    conn: &mut PgConnection,
    claim: &Claim,
    before: &Cursor,
    next: Option<(&str, &str)>,
) -> Result<(), MigrationError> {
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET owned_activity_kind=COALESCE($4,owned_activity_kind),owned_activity_source_id=COALESCE($5,owned_activity_source_id),owned_walk_complete=($4::text IS NULL) WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND owned_activity_kind IS NOT DISTINCT FROM $7 AND owned_activity_source_id IS NOT DISTINCT FROM $8 AND NOT owned_walk_complete AND source_walk_complete AND state='preparing' AND phase='classify'")
        .bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(next.map(|v|v.0)).bind(next.map(|v|v.1)).bind(claim.epoch).bind(&before.kind).bind(&before.source).execute(conn).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::Conflict);
    }
    Ok(())
}
async fn settle(conn: &mut PgConnection, claim: &Claim, token: Uuid) -> Result<(), MigrationError> {
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(claim.plan)
        .bind(token)
        .bind(claim.epoch)
        .execute(conn)
        .await?;
    Ok(())
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if p.get::<String, _>("family") != "activity"
        || p.get::<String, _>("phase") != "classify"
        || !p.get::<bool, _>("source_walk_complete")
    {
        return Err(MigrationError::Conflict);
    }
    if p.get::<bool, _>("owned_walk_complete") {
        return Ok(Progress::Finished);
    }
    let before = Cursor {
        kind: p.get("owned_activity_kind"),
        source: p.get("owned_activity_source_id"),
    };
    let next = sqlx::query("SELECT * FROM crm_family_refresh_next_owned_activity($1,$2,$3,$4)")
        .bind(claim.organization.0)
        .bind(claim.bundle)
        .bind(&before.kind)
        .bind(&before.source)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(next) = next else {
        checkpoint(&mut tx, claim, &before, None).await?;
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    let kind_name: String = next.get("kind");
    let source: String = next.get("source_id");
    let hash = activity_store::source_key(
        key,
        claim.organization,
        b.get("source_account_id"),
        &kind_name,
        source.as_bytes(),
    );
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4)").bind(claim.plan).bind(claim.organization.0).bind(&kind_name).bind(&hash).fetch_one(&mut *tx).await?;
    if exists {
        // The composite cursor is variable-width and must be settled even when
        // the observed identity already has an immutable outcome.
        let Some(token) = preparation::reserve(&mut tx, claim, policy, 4096).await? else {
            return Ok(Progress::Capacity);
        };
        checkpoint(&mut tx, claim, &before, Some((&kind_name, &source))).await?;
        settle(&mut tx, claim, token).await?;
        tx.commit().await?;
        return Ok(Progress::Advanced);
    }
    let cohort: Uuid = next.get("cohort_id");
    let person: Uuid = next.get("person_id");
    let kind: Kind =
        serde_json::from_value(serde_json::json!(kind_name)).map_err(|_| MigrationError::Crypto)?;
    drop(tx);
    let (baseline, reason) =
        match activity_baseline::discover(pool, key, claim, cohort, kind, &source).await? {
            Discovery::Proven(b) => (Some(b), Hold::SourceNotObserved),
            Discovery::Held(h) => (None, h),
        };
    let (mut tx, _, p) = preparation::begin(pool, claim).await?;
    if p.get::<Option<String>, _>("owned_activity_kind") != before.kind
        || p.get::<Option<String>, _>("owned_activity_source_id") != before.source
        || p.get::<bool, _>("owned_walk_complete")
    {
        return Err(MigrationError::Conflict);
    }
    let id = Uuid::new_v4();
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Activity,
        revision: p.get("revision"),
    };
    let proposal = MissingProposal {
        kind,
        source_id: source.clone(),
        source_not_observed: true,
        reason,
        baseline,
    };
    let sealed = scope.seal(key, id, Purpose::Manifest, &proposal)?;
    let bound =
        i64::try_from(sealed.ciphertext.len()).map_err(|_| MigrationError::StorageLimit)? + 16384;
    let Some(token) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Progress::Capacity);
    };
    let counts = Counts {
        units: 1,
        held: 1,
        ..Counts::default()
    };
    let old = p.get::<serde_json::Value, _>("counts");
    let totals: Counts = if old == serde_json::json!({}) {
        Counts::default()
    } else {
        serde_json::from_value(old).map_err(|_| MigrationError::Crypto)?
    };
    let totals = totals
        .checked_add(&counts)
        .filter(Counts::reconciles)
        .ok_or(MigrationError::Crypto)?;
    let position = p
        .get::<i64, _>("position")
        .checked_add(1)
        .ok_or(MigrationError::Crypto)?;
    let reason = serde_json::to_value(reason).map_err(|_| MigrationError::Crypto)?;
    let reason = reason.as_str().ok_or(MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,position,kind,source_id,source_key_hmac,person_id,disposition,reason,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'held',$11,$12,$13,$14,$15)").bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(cohort).bind(position).bind(&kind_name).bind(&source).bind(hash).bind(person).bind(reason).bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET position=$3,counts=$4 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(position).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
    checkpoint(&mut tx, claim, &before, Some((&kind_name, &source))).await?;
    settle(&mut tx, claim, token).await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,manifest_id=%id,reason,"Family refresh absent activity held");
    Ok(Progress::Advanced)
}

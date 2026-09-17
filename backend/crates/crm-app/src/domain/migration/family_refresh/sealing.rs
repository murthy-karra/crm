//! Bounded immutable native recipes followed by a keyed, ordered plan seal.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    model::{Counts, Family},
    native_proof, preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{crypto, snapshot::SnapshotPolicy, MigrationError},
};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;
#[derive(Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced,
    Finished,
    Capacity,
}
fn counts(value: Value) -> Result<Counts, MigrationError> {
    if value == json!({}) {
        Ok(Counts::default())
    } else {
        serde_json::from_value(value).map_err(|_| MigrationError::Crypto)
    }
}
fn digest(key: &RawPayloadKey, purpose: &str, value: &Value) -> Result<Vec<u8>, MigrationError> {
    Ok(crypto::request_digest(
        key,
        purpose,
        &serde_json::to_vec(value).map_err(|_| MigrationError::Crypto)?,
    )
    .to_vec())
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
pub(super) async fn settle_control(
    conn: &mut PgConnection,
    org: Uuid,
    bundle: Uuid,
    plan: Uuid,
) -> Result<(), MigrationError> {
    let r=sqlx::query("SELECT r.token,p.lease_epoch FROM migration_family_refresh_reservation r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.organization_id=r.organization_id WHERE r.plan_id=$1 AND r.organization_id=$2 AND r.purpose='control'").bind(plan).bind(org).fetch_one(&mut *conn).await?;
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,false)")
        .bind(org)
        .bind(bundle)
        .bind(plan)
        .bind(r.get::<Uuid, _>("token"))
        .bind(r.get::<i64, _>("lease_epoch"))
        .execute(conn)
        .await?;
    Ok(())
}
/// Terminal bundles cannot accept another control mutation. At most three
/// selected plans and the original shared payer retain cancellation capacity;
/// superseded non-payers release theirs during replanning.
pub(super) async fn release_terminal_controls(
    conn: &mut PgConnection,
    org: Uuid,
    bundle: Uuid,
) -> Result<(), MigrationError> {
    let rows=sqlx::query("SELECT r.token,r.plan_id,p.lease_epoch FROM migration_family_refresh_reservation r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.organization_id=r.organization_id JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE b.id=$1 AND b.organization_id=$2 AND b.state IN ('completed','cancelled') AND r.purpose='control' ORDER BY p.id LIMIT 5")
        .bind(bundle).bind(org).fetch_all(&mut *conn).await?;
    if rows.len() > 4 {
        return Err(MigrationError::Crypto);
    }
    for row in rows {
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,true)")
            .bind(org)
            .bind(bundle)
            .bind(row.get::<Uuid, _>("plan_id"))
            .bind(row.get::<Uuid, _>("token"))
            .bind(row.get::<i64, _>("lease_epoch"))
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
/// Bind ready families and the exact identities of excluded/unready groups. A
/// preparing family's partial chain never becomes a confirmation digest.
pub(super) async fn bundle_digest(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    org: Uuid,
    bundle: Uuid,
) -> Result<Vec<u8>, MigrationError> {
    let b:Value=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'organization',organization_id,'revision',revision::text,'parent',parent_import_id,'parent_plan',parent_plan_id,'source',encode(sha256(source_ciphertext),'hex'),'core',core_report_id,'history',history_capture_id) FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2").bind(bundle).bind(org).fetch_one(&mut *conn).await?;
    let plans:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('family',family,'id',id,'revision',revision::text,'digest',CASE WHEN state='ready' THEN encode(digest,'hex') END) ORDER BY family),'[]'::jsonb) FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state<>'superseded'").bind(bundle).bind(org).fetch_one(conn).await?;
    digest(key, "family-refresh-bundle-seal-v1", &json!([b, plans]))
}
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
) -> Result<Progress, MigrationError> {
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    if !p.get::<bool, _>("source_walk_complete") || !p.get::<bool, _>("owned_walk_complete") {
        return Err(MigrationError::ImportBusy);
    }
    let family: Family = serde_json::from_value(json!(p.get::<String, _>("family")))
        .map_err(|_| MigrationError::Crypto)?;
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family,
        revision: p.get("revision"),
    };
    if !p.get::<bool, _>("proofs_complete") {
        let next = p
            .get::<i64, _>("proof_after")
            .checked_add(1)
            .ok_or(MigrationError::Crypto)?;
        let row=sqlx::query("SELECT * FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND position=$3").bind(claim.plan).bind(claim.organization.0).bind(next).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            if p.get::<i64, _>("proof_after") != p.get::<i64, _>("position") {
                return Err(MigrationError::Crypto);
            }
            sqlx::query("UPDATE migration_family_refresh_plan SET proofs_complete=true WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(Progress::Advanced);
        };
        let recipes = native_proof::draft(&mut tx, key, claim, &b, &p, &row).await?;
        let mut sealed = Vec::new();
        let mut bound = 8192_i64;
        for recipe in recipes {
            let id = Uuid::new_v4();
            let value = scope.seal(key, id, Purpose::NativeProof, &recipe)?;
            bound = bound
                .checked_add(
                    i64::try_from(value.ciphertext.len())
                        .map_err(|_| MigrationError::StorageLimit)?
                        + 1024,
                )
                .ok_or(MigrationError::StorageLimit)?;
            sealed.push((id, recipe, value));
        }
        if sealed.is_empty() {
            sqlx::query("UPDATE migration_family_refresh_plan SET proof_after=$3 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(next).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(Progress::Advanced);
        }
        let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
            return Ok(Progress::Capacity);
        };
        for (id, recipe, value) in sealed {
            // Typed conversion happens inside PostgreSQL, preserving exact
            // NUMERIC scale without a floating-point JSON roundtrip.
            let table = recipe.table.name();
            let sql=format!("INSERT INTO migration_family_refresh_write_proof(id,bundle_id,plan_id,organization_id,manifest_id,table_name,operation,target_id,expected_revision,before_hash,after_hash,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,CASE WHEN $10::jsonb IS NULL THEN NULL ELSE crm_family_refresh_native_digest(to_jsonb(jsonb_populate_record(NULL::{table},$10))) END,CASE WHEN $11::jsonb IS NULL THEN NULL ELSE crm_family_refresh_native_digest(to_jsonb(jsonb_populate_record(NULL::{table},$11))) END,$12,$13)");
            sqlx::query(&sql)
                .bind(id)
                .bind(claim.bundle)
                .bind(claim.plan)
                .bind(claim.organization.0)
                .bind(row.get::<Uuid, _>("id"))
                .bind(table)
                .bind(recipe.operation())
                .bind(recipe.target)
                .bind(recipe.expected_revision)
                .bind(recipe.before)
                .bind(recipe.after)
                .bind(value.nonce.as_slice())
                .bind(value.ciphertext)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE migration_family_refresh_plan SET proof_after=$3 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(next).execute(&mut *tx).await?;
        settle(&mut tx, claim, reservation).await?;
        tx.commit().await?;
        return Ok(Progress::Advanced);
    }
    let next = p
        .get::<i64, _>("seal_after")
        .checked_add(1)
        .ok_or(MigrationError::Crypto)?;
    let row=sqlx::query("SELECT m.*,to_jsonb(m)-ARRAY['nonce','ciphertext'] AS header,encode(sha256(m.nonce||m.ciphertext),'hex') AS payload_hash FROM migration_family_refresh_manifest m WHERE plan_id=$1 AND organization_id=$2 AND position=$3").bind(claim.plan).bind(claim.organization.0).bind(next).fetch_optional(&mut *tx).await?;
    let seed = digest(
        key,
        "family-refresh-plan-seal-v1",
        &json!([
            claim.organization.0,
            claim.bundle,
            claim.plan,
            family,
            p.get::<i64, _>("revision"),
            p.get::<Vec<u8>, _>("nonce"),
            p.get::<Vec<u8>, _>("ciphertext")
        ]),
    )?;
    let previous = p.get::<Option<Vec<u8>>, _>("digest").unwrap_or(seed);
    let Some(row) = row else {
        let totals = counts(p.get("counts"))?;
        let sealed_counts = counts(p.get("seal_counts"))?;
        if totals != sealed_counts
            || p.get::<i64, _>("seal_after") != p.get::<i64, _>("position")
            || !totals.reconciles()
        {
            return Err(MigrationError::Crypto);
        }
        sqlx::query("UPDATE migration_family_refresh_plan SET state='ready',digest=$3,counts=$4,seal_counts=$4,expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(previous).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).execute(&mut *tx).await?;
        let combined = bundle_digest(&mut tx, key, claim.organization.0, claim.bundle).await?;
        sqlx::query("UPDATE migration_family_refresh_bundle SET digest=$3,state=CASE WHEN NOT EXISTS(SELECT 1 FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state NOT IN ('ready','cancelled','superseded')) THEN 'ready' ELSE 'preparing' END,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(claim.bundle).bind(claim.organization.0).bind(combined).execute(&mut *tx).await?;
        settle_control(&mut tx, claim.organization.0, claim.bundle, claim.plan).await?;
        let payer: Uuid = b.get("payer_plan_id");
        if payer != claim.plan {
            settle_control(&mut tx, claim.organization.0, claim.bundle, payer).await?;
        }
        tx.commit().await?;
        return Ok(Progress::Finished);
    };
    let _: Value = scope.open(
        key,
        row.get("id"),
        Purpose::Manifest,
        row.get("nonce"),
        row.get("ciphertext"),
    )?;
    let proofs:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg((to_jsonb(w)-ARRAY['nonce','ciphertext'])||jsonb_build_object('payload_hash',encode(sha256(nonce||ciphertext),'hex')) ORDER BY id),'[]'::jsonb) FROM migration_family_refresh_write_proof w WHERE manifest_id=$1 AND plan_id=$2 AND organization_id=$3").bind(row.get::<Uuid,_>("id")).bind(claim.plan).bind(claim.organization.0).fetch_one(&mut *tx).await?;
    let totals = counts(p.get("seal_counts"))?
        .checked_add(&counts(row.get("counts"))?)
        .filter(Counts::reconciles)
        .ok_or(MigrationError::Crypto)?;
    let hash = digest(
        key,
        "family-refresh-plan-seal-v1",
        &json!([
            previous,
            row.get::<Value, _>("header"),
            row.get::<String, _>("payload_hash"),
            proofs
        ]),
    )?;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, 8192).await? else {
        return Ok(Progress::Capacity);
    };
    sqlx::query("UPDATE migration_family_refresh_plan SET seal_after=$3,seal_counts=$4,digest=$5 WHERE id=$1 AND organization_id=$2").bind(claim.plan).bind(claim.organization.0).bind(next).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).bind(hash).execute(&mut *tx).await?;
    settle(&mut tx, claim, reservation).await?;
    tx.commit().await?;
    Ok(Progress::Advanced)
}

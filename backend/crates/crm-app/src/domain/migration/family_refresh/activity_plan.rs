//! Persist one activity outcome, including authenticated source/mapping/baseline
//! evidence. This grants no native permit and does not advance an ownership head.
use super::{
    activity_baseline, activity_delta, activity_mapping,
    cohort::Claim,
    core_resolution::{self, Resolution},
    evidence::{Purpose, Scope},
    model::{Baseline, Counts, Current, Family, Hold, Kind},
    new_identity, preparation,
};
use crate::{
    config::RawPayloadKey,
    domain::migration::{activity_store, snapshot::SnapshotPolicy, MigrationError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Decision {
    Ready {
        baseline: Option<Box<Baseline>>,
        source: Box<activity_mapping::Evidence>,
        native: Box<activity_delta::Proposal>,
    },
    Held {
        reason: Hold,
    },
}
#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub kind: Kind,
    pub source_id: String,
    pub source_row: Uuid,
    pub source_person: String,
    pub semantic: Vec<u8>,
    pub decision: Decision,
}
pub enum Prepared {
    Unit(Uuid),
    Held(Hold),
    Capacity,
}

pub async fn prepare_unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    cohort: Uuid,
    kind: Kind,
    source_id: &str,
) -> Result<Prepared, MigrationError> {
    prepare(
        pool,
        key,
        policy,
        claim,
        Unit {
            cohort,
            kind,
            source_id,
            walk: None,
        },
    )
    .await
}
pub(super) struct Unit<'a> {
    pub cohort: Uuid,
    pub kind: Kind,
    pub source_id: &'a str,
    pub walk: Option<super::activity_walk::Position>,
}
pub(super) async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    claim: &Claim,
    input: Unit<'_>,
) -> Result<Prepared, MigrationError> {
    let Unit {
        cohort,
        kind,
        source_id,
        walk,
    } = input;
    let (kind_name, source_kind) = match kind {
        Kind::Note => ("note", core_resolution::Kind::Note),
        Kind::Task => ("task", core_resolution::Kind::Task),
        _ => return Err(MigrationError::InvalidInput),
    };
    let selected = match core_resolution::resolve(pool, key, claim, source_kind, source_id).await? {
        Resolution::Ready(r) => r,
        Resolution::Held(h) => return Ok(Prepared::Held(h)),
    };
    let source_person = selected
        .source_person
        .as_ref()
        .ok_or(MigrationError::Crypto)?;
    let (account, person, consumed) = {
        let (mut tx, b, p) = preparation::begin(pool, claim).await?;
        if p.get::<String, _>("family") != "activity" || !p.get::<bool, _>("mappings_complete") {
            return Err(MigrationError::ImportBusy);
        }
        let c=sqlx::query("SELECT person_id,source_person_id FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(cohort).bind(claim.bundle).bind(claim.organization.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
        if c.get::<String, _>("source_person_id") != *source_person {
            return Ok(Prepared::Held(Hold::IdentityMismatch));
        }
        let account: i64 = b.get("source_account_id");
        let hash = activity_store::source_key(
            key,
            claim.organization,
            account,
            kind_name,
            source_id.as_bytes(),
        );
        if let Some(row)=sqlx::query("SELECT id,cohort_id,source_row_id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4").bind(claim.plan).bind(claim.organization.0).bind(kind_name).bind(&hash).fetch_optional(&mut *tx).await? {
            if row.get::<Option<Uuid>,_>("cohort_id")!=Some(cohort) || row.get::<Option<Uuid>,_>("source_row_id")!=Some(selected.row) {return Err(MigrationError::Crypto);}
            super::activity_walk::advance(&mut tx,claim,walk).await?;
            let id=row.get("id");
            tx.commit().await?;
            return Ok(Prepared::Unit(id));
        }
        let consumed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_activity_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_id=$4)").bind(claim.organization.0).bind(account).bind(kind_name).bind(source_id).fetch_one(&mut *tx).await?;
        (account, c.get::<Uuid, _>("person_id"), consumed)
    };
    let converted = activity_mapping::convert(pool, key, claim, kind, source_id).await?;
    let mut baseline = None;
    let mut held = match &converted {
        activity_mapping::Conversion::Held(h) => Some(*h),
        _ => None,
    };
    if held.is_none() {
        if consumed {
            match activity_baseline::discover(pool, key, claim, cohort, kind, source_id).await? {
                activity_baseline::Discovery::Proven(b) => baseline = Some(b),
                activity_baseline::Discovery::Held(h) => held = Some(h),
            }
        } else {
            match new_identity::discover(pool, key, claim, cohort, kind, source_id).await? {
                new_identity::Discovery::New(candidate)
                    if candidate.person == person && candidate.source == selected.row => {}
                new_identity::Discovery::New(_) => return Err(MigrationError::Crypto),
                new_identity::Discovery::Held(h) => held = Some(h),
            }
        }
    }
    let (mut tx, b, p) = preparation::begin(pool, claim).await?;
    let hash = activity_store::source_key(
        key,
        claim.organization,
        account,
        kind_name,
        source_id.as_bytes(),
    );
    // The claim may be shared by duplicate callers. Serialize allocation and
    // settlement, and preserve an earlier immutable hold or prospective ID.
    if let Some(id)=sqlx::query_scalar("SELECT id FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key_hmac=$4").bind(claim.plan).bind(claim.organization.0).bind(kind_name).bind(&hash).fetch_optional(&mut *tx).await? {super::activity_walk::advance(&mut tx,claim,walk).await?;tx.commit().await?;return Ok(Prepared::Unit(id));}
    let mut native = None;
    let mut converted_evidence = None;
    if let (None, activity_mapping::Conversion::Ready(converted)) = (held, converted) {
        let native_key = format!("v1:{account}:{source_id}");
        let scope = activity_delta::Scope {
            organization: claim.organization.0,
            source_external_id: &native_key,
            correlation: claim.bundle,
        };
        let heads:Vec<Uuid>=sqlx::query_scalar("SELECT result_id FROM migration_family_refresh_head WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key_hmac=$4 LIMIT 2").bind(claim.organization.0).bind(account).bind(kind_name).bind(&hash).fetch_all(&mut *tx).await?;
        let result = if let Some(baseline) = baseline.as_ref() {
            let sql = if kind == Kind::Note {
                "SELECT to_jsonb(n) FROM note n WHERE id=$1 AND organization_id=$2"
            } else {
                "SELECT to_jsonb(t) FROM task t WHERE id=$1 AND organization_id=$2"
            };
            let row: Option<serde_json::Value> = sqlx::query_scalar(sql)
                .bind(baseline.target_id)
                .bind(claim.organization.0)
                .fetch_optional(&mut *tx)
                .await?;
            let current = row.map(|row| Current {
                person_id: person,
                target_id: baseline.target_id,
                revision: row["revision"].as_i64().unwrap_or(0),
                deleted: !row["deleted_at"].is_null(),
                head_id: heads.first().copied(),
                native: row,
            });
            if heads.len() > 1 {
                Err(Hold::IdentityMismatch)
            } else {
                activity_delta::propose_update(
                    scope,
                    Some(baseline),
                    current.as_ref(),
                    &converted.evidence.source,
                    &converted.members,
                    true,
                )
            }
        } else if !heads.is_empty() {
            Err(Hold::BaselineUnproven)
        } else {
            activity_delta::propose_insert(
                scope,
                person,
                Uuid::new_v4(),
                &converted.evidence.source,
                &converted.members,
                false,
                true,
            )
        };
        match result {
            Ok(value) => {
                native = Some(value);
                converted_evidence = Some(converted.evidence);
            }
            Err(h) => held = Some(h),
        }
    }
    let (
        mut decision,
        mut counts,
        disposition,
        target,
        expected_head,
        expected_revision,
        baseline_result,
    ) = if let Some(reason) = held {
        (
            Decision::Held { reason },
            Counts {
                units: 1,
                held: 1,
                ..Counts::default()
            },
            "held",
            None,
            None,
            None,
            None,
        )
    } else {
        let native = native.ok_or(MigrationError::Crypto)?;
        let source = converted_evidence.ok_or(MigrationError::Crypto)?;
        let counts = native.counts.clone();
        let disposition = if counts.inserts == 1 {
            "insert"
        } else if counts.updates == 1 {
            "update"
        } else if counts.already_current == 1 {
            "already_current"
        } else {
            return Err(MigrationError::Crypto);
        };
        let target = Some(native.target);
        let head = native.expected_head;
        let revision = Some(native.expected_revision);
        let result = baseline.as_ref().map(|b| b.result_id);
        (
            Decision::Ready {
                baseline: baseline.map(Box::new),
                source: Box::new(source),
                native: Box::new(native),
            },
            counts,
            disposition,
            target,
            head,
            revision,
            result,
        )
    };
    if let Decision::Ready { source, .. } = &decision {
        counts.source_only = source
            .source_only
            .values()
            .try_fold(0_u64, |a, b| a.checked_add(*b))
            .ok_or(MigrationError::StorageLimit)?;
    }
    if let Decision::Ready { native, .. } = &mut decision {
        native.counts = counts.clone();
    }
    let id = Uuid::new_v4();
    let evidence = Evidence {
        kind,
        source_id: source_id.into(),
        source_row: selected.row,
        source_person: source_person.clone(),
        semantic: selected.semantic,
        decision,
    };
    let scope = Scope {
        organization: claim.organization,
        bundle: claim.bundle,
        plan: claim.plan,
        family: Family::Activity,
        revision: p.get("revision"),
    };
    let sealed = scope.seal(key, id, Purpose::Manifest, &evidence)?;
    let bound = i64::try_from(sealed.ciphertext.len())
        .map_err(|_| MigrationError::StorageLimit)?
        .checked_add(16384)
        .ok_or(MigrationError::StorageLimit)?;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Prepared::Capacity);
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
    let reason = held
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| MigrationError::Crypto)?;
    sqlx::query("INSERT INTO migration_family_refresh_manifest(id,bundle_id,plan_id,organization_id,cohort_id,source_row_id,position,kind,source_key_hmac,person_id,target_id,baseline_result_id,expected_head_id,expected_revision,disposition,reason,counts,nonce,ciphertext,added_byte_bound) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)")
        .bind(id).bind(claim.bundle).bind(claim.plan).bind(claim.organization.0).bind(cohort).bind(selected.row).bind(position).bind(kind_name).bind(hash).bind(person).bind(target).bind(baseline_result).bind(expected_head).bind(expected_revision).bind(disposition).bind(reason.as_ref().and_then(|v|v.as_str())).bind(serde_json::to_value(counts).map_err(|_|MigrationError::Crypto)?).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(bound).execute(&mut *tx).await?;
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET position=$4,counts=$5,phase='classify' WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_epoch=$6 AND lease_expires_at>clock_timestamp() AND state='preparing'").bind(claim.plan).bind(claim.organization.0).bind(claim.token).bind(position).bind(serde_json::to_value(totals).map_err(|_|MigrationError::Crypto)?).bind(claim.epoch).execute(&mut *tx).await?.rows_affected();
    if changed != 1 || b.get::<i64, _>("source_account_id") != account {
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
    super::activity_walk::advance(&mut tx, claim, walk).await?;
    tx.commit().await?;
    tracing::info!(organization_id=%claim.organization,plan_id=%claim.plan,manifest_id=%id,disposition,"Family refresh activity proposal prepared");
    Ok(Prepared::Unit(id))
}

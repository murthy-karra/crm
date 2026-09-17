//! One confirmed note/task unit with immutable source and exact native proofs.
use super::{
    activity_delta::{self, Members},
    activity_mapping, activity_plan,
    cohort::Claim,
    evidence::{Purpose, Scope},
    execution::{self, Progress, ResultUnit},
    mapping_selection::Destination,
    model::{Counts, Current, Hold, Kind},
    native_baseline::{AfterState, ResultData, State},
    native_write, preparation, source_policy,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::migration::{snapshot::SnapshotPolicy, MigrationError},
};
use sqlx::{postgres::PgRow, PgConnection, PgPool, Row};
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub(super) async fn apply(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: &ReleaseReadiness,
    claim: &Claim,
    scope: Scope,
    unit: PgRow,
) -> Result<Progress, MigrationError> {
    let disposition = unit.get::<String, _>("disposition");
    let mut held = None;
    let mut approved = None;
    let mut converted = None;
    if !matches!(disposition.as_str(), "held" | "excluded") {
        let saved: activity_plan::Evidence = scope.open(
            key,
            unit.get("id"),
            Purpose::Manifest,
            unit.get("nonce"),
            unit.get("ciphertext"),
        )?;
        let activity_plan::Decision::Ready { source, .. } = &saved.decision else {
            return Err(MigrationError::Crypto);
        };
        match activity_mapping::convert(pool, key, claim, saved.kind, &saved.source_id).await? {
            activity_mapping::Conversion::Held(h) => held = Some(h),
            activity_mapping::Conversion::Ready(current) => {
                if serde_json::to_value(&current.evidence).map_err(|_| MigrationError::Crypto)?
                    != serde_json::to_value(source).map_err(|_| MigrationError::Crypto)?
                {
                    held = Some(Hold::SourceConflict)
                }
                converted = Some(current);
            }
        }
        approved = Some(saved);
    }
    let members: Vec<Uuid> = converted
        .as_ref()
        .map(|c| c.members.all.iter().copied().collect())
        .unwrap_or_default();
    let (mut tx, b, p) = preparation::read_begin_members(pool, claim, &members).await?;
    if p.get::<String, _>("state") != "running"
        || p.get::<String, _>("phase") != "apply"
        || p.get::<i64, _>("apply_position") + 1 != unit.get::<i64, _>("position")
        || p.get::<i64, _>("revision") != scope.revision
    {
        return Err(MigrationError::Conflict);
    }
    release
        .require_family_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let bound = unit
        .get::<i64, _>("added_byte_bound")
        .checked_mul(2)
        .and_then(|n| n.checked_add(16384))
        .filter(|n| *n <= 64 * 1024 * 1024)
        .ok_or(MigrationError::StorageLimit)?;
    let Some(reservation) = preparation::reserve(&mut tx, claim, policy, bound).await? else {
        return Ok(Progress::Capacity);
    };
    let result_id = Uuid::new_v4();
    let result = if let Some(reason) = held {
        ResultUnit::held(&unit, reason)
    } else if matches!(disposition.as_str(), "held" | "excluded") {
        ResultUnit {
            disposition: if disposition == "held" {
                "held"
            } else {
                "excluded"
            },
            reason: None,
            person: unit.get("person_id"),
            target: unit.get("target_id"),
            revision: None,
            counts: Counts {
                units: 1,
                held: u64::from(disposition == "held"),
                excluded: u64::from(disposition == "excluded"),
                ..Counts::default()
            },
            data: ResultData { after_state: None },
            native_bytes: 0,
        }
    } else {
        execute(
            &mut tx,
            key,
            claim,
            scope,
            &b,
            &p,
            &unit,
            approved.as_ref().ok_or(MigrationError::Crypto)?,
            converted.as_deref().ok_or(MigrationError::Crypto)?,
        )
        .await?
    };
    execution::finish(
        tx,
        key,
        claim,
        scope,
        &p,
        &unit,
        result_id,
        reservation,
        result,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn execute(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    scope: Scope,
    b: &PgRow,
    p: &PgRow,
    unit: &PgRow,
    saved: &activity_plan::Evidence,
    converted: &activity_mapping::Converted,
) -> Result<ResultUnit, MigrationError> {
    let activity_plan::Decision::Ready {
        baseline, native, ..
    } = &saved.decision
    else {
        return Err(MigrationError::Crypto);
    };
    let table = match saved.kind {
        Kind::Note => "note",
        Kind::Task => "task",
        _ => return Err(MigrationError::Crypto),
    };
    let mut members = Members {
        all: Default::default(),
        active: Default::default(),
    };
    for mapping in &converted.evidence.mappings {
        if let Some(Destination::Member { id, label, status }) = &mapping.destination {
            let row=sqlx::query("SELECT m.status,u.display_name FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.user_id=$2")
                .bind(claim.organization.0).bind(id).fetch_optional(&mut *conn).await?;
            let Some(row) = row else {
                return Ok(ResultUnit::held(unit, Hold::TargetUnavailable));
            };
            if row.get::<String, _>("status") != *status
                || row.get::<String, _>("display_name") != *label
            {
                return Ok(ResultUnit::held(unit, Hold::TargetUnavailable));
            }
            members.all.insert(*id);
            if status == "active" {
                members.active.insert(*id);
            }
        }
    }
    let c=sqlx::query("SELECT * FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(unit.get::<Uuid,_>("cohort_id")).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *conn).await?;
    let person:Option<Uuid>=sqlx::query_scalar("SELECT p.id FROM person p JOIN migration_import_identity i ON i.target_id=p.id AND i.organization_id=p.organization_id AND i.family='people' WHERE p.id=$1 AND p.organization_id=$2 AND i.source_account_id=$3 AND i.source_id=$4 FOR UPDATE OF p").bind(native.person).bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(c.get::<String,_>("source_person_id")).fetch_optional(&mut *conn).await?;
    if person.is_none() {
        return Ok(ResultUnit::held(unit, Hold::TargetErased));
    }
    if let Err(h) = source_policy::qualify_accepted_scan(conn, claim.organization, b, p, &c).await?
    {
        return Ok(ResultUnit::held(unit, h));
    }
    if c.get::<String, _>("source_person_id") != saved.source_person {
        return Ok(ResultUnit::held(unit, Hold::IdentityMismatch));
    }
    let head:Option<Uuid>=sqlx::query_scalar("SELECT result_id FROM migration_family_refresh_head WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_key_hmac=$4 FOR UPDATE")
        .bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(table).bind(unit.get::<Vec<u8>,_>("source_key_hmac")).fetch_optional(&mut *conn).await?;
    if head != native.expected_head || head != unit.get::<Option<Uuid>, _>("expected_head_id") {
        return Ok(ResultUnit::held(unit, Hold::StaleHead));
    }
    let identity:Option<Uuid>=sqlx::query_scalar("SELECT target_id FROM migration_activity_identity WHERE organization_id=$1 AND source_account_id=$2 AND kind=$3 AND source_id=$4")
        .bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(table).bind(&saved.source_id).fetch_optional(&mut *conn).await?;
    let sql = format!(
        "SELECT to_jsonb(n) FROM {table} n WHERE n.id=$1 AND n.organization_id=$2 FOR UPDATE"
    );
    let current: Option<serde_json::Value> = sqlx::query_scalar(&sql)
        .bind(native.target)
        .bind(claim.organization.0)
        .fetch_optional(&mut *conn)
        .await?;
    let native_key = format!(
        "v1:{}:{}",
        b.get::<i64, _>("source_account_id"),
        saved.source_id
    );
    let delta_scope = activity_delta::Scope {
        organization: claim.organization.0,
        source_external_id: &native_key,
        correlation: b
            .get::<Option<Uuid>, _>("remainder_origin_bundle_id")
            .unwrap_or(claim.bundle),
    };
    let proposal = if let Some(baseline) = baseline {
        if identity != Some(native.target) {
            return Ok(ResultUnit::held(unit, Hold::IdentityMismatch));
        }
        let current = current.map(|row| Current {
            person_id: native.person,
            target_id: native.target,
            revision: row["revision"].as_i64().unwrap_or(0),
            deleted: !row["deleted_at"].is_null(),
            head_id: head,
            native: row,
        });
        activity_delta::propose_update(
            delta_scope,
            Some(baseline),
            current.as_ref(),
            &converted.evidence.source,
            &members,
            true,
        )
    } else {
        if let Err(h) = super::first_coverage::qualify(
            conn,
            claim.organization,
            b,
            p,
            &c,
            super::model::Family::Activity,
        )
        .await?
        {
            return Ok(ResultUnit::held(unit, h));
        }
        let sql=format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE organization_id=$1 AND source='fub' AND source_external_id IN ($2,$3))");
        let collision: bool = sqlx::query_scalar(&sql)
            .bind(claim.organization.0)
            .bind(&saved.source_id)
            .bind(&native_key)
            .fetch_one(&mut *conn)
            .await?;
        activity_delta::propose_insert(
            delta_scope,
            native.person,
            native.target,
            &converted.evidence.source,
            &members,
            identity.is_some() || current.is_some() || collision,
            true,
        )
    };
    let mut proposal = match proposal {
        Ok(p) => p,
        Err(h) => return Ok(ResultUnit::held(unit, h)),
    };
    // Preparation adds retained source-only coverage to the native action
    // counts. Recompute it from the same authenticated conversion before the
    // complete frozen-proposal comparison.
    proposal.counts.source_only = converted
        .evidence
        .source_only
        .values()
        .try_fold(0_u64, |total, count| total.checked_add(*count))
        .ok_or(MigrationError::Crypto)?;
    if serde_json::to_value(&proposal).map_err(|_| MigrationError::Crypto)?
        != serde_json::to_value(native).map_err(|_| MigrationError::Crypto)?
    {
        return Ok(ResultUnit::held(unit, Hold::LocalChange));
    }
    let proofs = native_write::load(conn, key, scope, unit.get("id")).await?;
    let writing = unit.get::<String, _>("disposition") != "already_current";
    if proofs.len() != usize::from(writing) {
        return Err(MigrationError::Crypto);
    }
    let mut native_bytes = 0;
    for proof in &proofs {
        if !native_write::current_matches(conn, claim, proof).await? {
            return Ok(ResultUnit::held(unit, Hold::LocalChange));
        }
        native_bytes += native_write::growth(conn, proof).await?;
        native_write::apply(conn, claim, proof).await?;
    }
    if baseline.is_none() {
        sqlx::query("INSERT INTO migration_activity_identity(organization_id,source_account_id,kind,source_id,target_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(claim.organization.0).bind(b.get::<i64,_>("source_account_id")).bind(table).bind(&saved.source_id).bind(native.target).bind(claim.bundle).bind(claim.plan).bind(unit.get::<Uuid,_>("id")).execute(&mut *conn).await?;
    }
    let sql = format!("SELECT to_jsonb(n) FROM {table} n WHERE n.id=$1 AND n.organization_id=$2");
    let after: serde_json::Value = sqlx::query_scalar(&sql)
        .bind(native.target)
        .bind(claim.organization.0)
        .fetch_one(&mut *conn)
        .await?;
    let revision = after["revision"].as_i64().ok_or(MigrationError::Crypto)?;
    let expected = if baseline.is_none() {
        1
    } else {
        native.expected_revision + i64::from(writing)
    };
    if revision != expected {
        return Err(MigrationError::Crypto);
    }
    let state = if table == "note" {
        State::Note {
            native: after,
            owned: true,
        }
    } else {
        State::Task {
            native: after,
            owned: true,
        }
    };
    Ok(ResultUnit {
        disposition: if writing {
            "applied"
        } else {
            "already_current"
        },
        reason: None,
        person: Some(native.person),
        target: Some(native.target),
        revision: Some(revision),
        counts: native.counts.clone(),
        native_bytes,
        data: ResultData {
            after_state: Some(AfterState {
                version: 1,
                manifest: unit.get("id"),
                source_id: saved.source_id.clone(),
                person: native.person,
                target: native.target,
                state,
            }),
        },
    })
}

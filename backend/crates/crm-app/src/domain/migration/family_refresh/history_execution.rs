//! Apply one immutable metadata-only history fact or correction with its result.
use super::{
    cohort::Claim,
    evidence::{Purpose, Scope},
    execution::{self, Progress, ResultUnit},
    history_baseline, history_plan, history_resolution,
    model::{Counts, Family, HistoryChange, Hold, Kind},
    native_baseline::ResultData,
    preparation, source_policy,
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
    let disposition: String = unit.get("disposition");
    let mut held = None;
    let mut saved = None;
    if !matches!(disposition.as_str(), "held" | "excluded") {
        let proposal: history_plan::Proposal = scope.open(
            key,
            unit.get("id"),
            Purpose::Manifest,
            unit.get("nonce"),
            unit.get("ciphertext"),
        )?;
        match history_resolution::resolve(pool, key, claim, proposal.kind, &proposal.source_id)
            .await?
        {
            history_resolution::Resolution::Held(reason) => held = Some(reason),
            history_resolution::Resolution::Ready(selected) => {
                if Some(selected.row) != unit.get::<Option<Uuid>, _>("source_row_id")
                    || serde_json::to_value(&selected.evidence)
                        .map_err(|_| MigrationError::Crypto)?
                        != serde_json::to_value(&proposal.source)
                            .map_err(|_| MigrationError::Crypto)?
                {
                    held = Some(Hold::SourceConflict);
                }
            }
        }
        saved = Some(proposal);
    }
    let (mut tx, b, p) = execution::begin(pool, claim).await?;
    if p.get::<i64, _>("revision") != scope.revision
        || p.get::<i64, _>("apply_position") + 1 != unit.get::<i64, _>("position")
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
    if held.is_none() {
        if let Some(proposal) = &saved {
            held = revalidate(&mut tx, key, claim, &b, &p, &unit, proposal).await?;
        }
    }
    let result_id = Uuid::new_v4();
    let result = if let Some(reason) = held {
        ResultUnit::held(&unit, reason)
    } else {
        ResultUnit {
            disposition: match disposition.as_str() {
                "held" => "held",
                "excluded" => "excluded",
                "already_current" => "already_current",
                _ => "applied",
            },
            reason: None,
            person: unit.get("person_id"),
            target: unit.get("target_id"),
            revision: None,
            counts: serde_json::from_value::<Counts>(unit.get("counts"))
                .map_err(|_| MigrationError::Crypto)?,
            data: ResultData { after_state: None },
            native_bytes: 0,
        }
    };
    execution::insert_result(&mut tx, key, claim, scope, &unit, result_id, &result).await?;
    if matches!(result.disposition, "applied" | "already_current") {
        write(
            &mut tx,
            key,
            claim,
            scope,
            &b,
            &p,
            &unit,
            saved.as_ref().ok_or(MigrationError::Crypto)?,
            result_id,
        )
        .await?;
    }
    execution::finish_inserted(tx, claim, &p, &unit, result_id, reservation, result).await
}
#[allow(clippy::too_many_arguments)]
async fn revalidate(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    b: &PgRow,
    p: &PgRow,
    unit: &PgRow,
    saved: &history_plan::Proposal,
) -> Result<Option<Hold>, MigrationError> {
    let kind = match saved.kind {
        Kind::Event => "event",
        Kind::Call => "call",
        Kind::Text => "text",
        _ => return Err(MigrationError::Crypto),
    };
    if unit.get::<String, _>("kind") != kind
        || unit.get::<Option<Uuid>, _>("target_id") != Some(saved.target)
        || unit.get::<Vec<u8>, _>("source_key_hmac") != saved.source.identity_hmac
    {
        return Err(MigrationError::Crypto);
    }
    let c=sqlx::query("SELECT * FROM migration_family_refresh_cohort WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(unit.get::<Uuid,_>("cohort_id")).bind(claim.bundle).bind(claim.organization.0).fetch_one(&mut *conn).await?;
    let person: Uuid = c.get("person_id");
    let live:Option<Uuid>=sqlx::query_scalar("SELECT person.id FROM person JOIN migration_import_identity i ON i.organization_id=person.organization_id AND i.target_id=person.id AND i.family='people' WHERE person.organization_id=$1 AND person.id=$2 AND i.source_account_id=$3 AND i.source_id=$4 AND i.import_id=$5 AND ((i.admission_id IS NULL AND EXISTS(SELECT 1 FROM migration_import_result r WHERE r.id=$6 AND r.organization_id=i.organization_id AND r.import_id=i.import_id AND r.plan_id=i.plan_id AND r.manifest_id=i.manifest_id AND r.person_id=i.target_id AND r.source_id=i.source_id AND r.disposition IN ('imported','already_imported'))) OR i.admission_result_id=$7) FOR UPDATE OF person")
        .bind(claim.organization.0).bind(person).bind(b.get::<i64,_>("source_account_id")).bind(c.get::<String,_>("source_person_id")).bind(b.get::<Uuid,_>("parent_import_id")).bind(c.get::<Option<Uuid>,_>("original_result_id")).bind(c.get::<Option<Uuid>,_>("admission_result_id")).fetch_optional(&mut *conn).await?;
    if live.is_none() {
        return Ok(Some(Hold::TargetErased));
    }
    if unit.get::<Option<Uuid>, _>("person_id") != Some(person)
        || saved.source.source_person != c.get::<String, _>("source_person_id")
    {
        return Ok(Some(Hold::IdentityMismatch));
    }
    if let Err(h) = source_policy::qualify_accepted_scan(conn, claim.organization, b, p, &c).await?
    {
        return Ok(Some(h));
    }
    if let Err(h) = source_policy::qualify_history_creation(conn, claim.organization, p, &c).await?
    {
        return Ok(Some(h));
    }
    let identity=sqlx::query("SELECT id FROM migration_history_import_identity WHERE organization_id=$1 AND identity_hmac=$2 FOR UPDATE").bind(claim.organization.0).bind(saved.source.identity_hmac.as_slice()).fetch_optional(&mut *conn).await?;
    sqlx::query("SELECT identity_id FROM migration_family_refresh_history_head WHERE organization_id=$1 AND identity_id=$2 FOR UPDATE").bind(claim.organization.0).bind(saved.identity).fetch_optional(&mut *conn).await?;
    if let Some(baseline) = &saved.baseline {
        if identity.as_ref().map(|i| i.get::<Uuid, _>("id")) != Some(saved.identity) {
            return Ok(Some(Hold::IdentityMismatch));
        }
        let current = match history_baseline::discover_in(
            conn,
            key,
            claim,
            c.get("id"),
            saved.kind,
            &saved.source.identity_hmac,
            b,
            p,
        )
        .await?
        {
            history_baseline::Discovery::Held(h) => return Ok(Some(h)),
            history_baseline::Discovery::Proven(b) => b,
        };
        if serde_json::to_value(&current).map_err(|_| MigrationError::Crypto)?
            != serde_json::to_value(baseline).map_err(|_| MigrationError::Crypto)?
            || current.result != unit.get::<Option<Uuid>, _>("expected_head_id")
            || Some(current.version) != unit.get::<Option<i64>, _>("expected_revision")
        {
            return Ok(Some(Hold::StaleHead));
        }
    } else {
        if identity.is_some() {
            return Ok(Some(Hold::BaselineUnproven));
        }
        if unit.get::<Option<Uuid>, _>("expected_head_id").is_some()
            || unit.get::<Option<i64>, _>("expected_revision").is_some()
        {
            return Err(MigrationError::Crypto);
        }
        if let Err(h) =
            super::first_coverage::qualify(conn, claim.organization, b, p, &c, Family::History)
                .await?
        {
            return Ok(Some(h));
        }
    }
    let change = super::model::compare_history(
        saved
            .baseline
            .as_ref()
            .map(|b| (b.semantic.as_slice(), b.person)),
        &saved.source.semantic_hmac,
        person,
        true,
        false,
        false,
    );
    if change != saved.change {
        return Err(MigrationError::Crypto);
    }
    let disposition = match change {
        HistoryChange::New => "insert",
        HistoryChange::AlreadyCurrent => "already_current",
        HistoryChange::Correction => "correction",
        HistoryChange::Held(h) => return Ok(Some(h)),
    };
    if disposition != unit.get::<String, _>("disposition") {
        return Err(MigrationError::Crypto);
    }
    Ok(None)
}
#[allow(clippy::too_many_arguments)]
async fn write(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    claim: &Claim,
    scope: Scope,
    b: &PgRow,
    p: &PgRow,
    unit: &PgRow,
    saved: &history_plan::Proposal,
    result: Uuid,
) -> Result<(), MigrationError> {
    let (stem, family) = match saved.kind {
        Kind::Event => ("event", "events"),
        Kind::Call => ("call", "calls"),
        Kind::Text => ("text", "text_messages"),
        _ => return Err(MigrationError::Crypto),
    };
    let person: Uuid = unit.get("person_id");
    let manifest: Uuid = unit.get("id");
    let capture: Uuid = p.get("history_capture_id");
    let time_basis = if saved.source.created.is_some() {
        "fub_record_created"
    } else {
        "unknown"
    };
    if saved.change == HistoryChange::New {
        sqlx::query("INSERT INTO migration_history_import_identity(id,organization_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id,identity_hmac,semantic_hmac,person_id,fact_id,family) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(saved.identity).bind(claim.organization.0).bind(claim.bundle).bind(claim.plan).bind(manifest).bind(saved.source.identity_hmac.as_slice()).bind(saved.source.semantic_hmac.as_slice()).bind(person).bind(saved.target).bind(family).execute(&mut *conn).await?;
        let display = saved.source.display.seal(scope, key, manifest)?;
        sqlx::query("INSERT INTO migration_history_import_display(id,organization_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$1,$5,$6)").bind(manifest).bind(claim.organization.0).bind(claim.bundle).bind(claim.plan).bind(display.nonce.as_slice()).bind(display.ciphertext).execute(&mut *conn).await?;
        let sql=format!("INSERT INTO fub_{stem}_record_imported(id,organization_id,identity_id,person_id,actor_kind,actor_user_id,origin,occurred_at,correlation_id,refresh_bundle_id,refresh_plan_id,refresh_manifest_id,source_created_at,source_time_basis,stable_position) VALUES($1,$2,$3,$4,'user',$5,'migration',clock_timestamp(),$6,$6,$7,$8,$9,$10,$11)");
        sqlx::query(&sql)
            .bind(saved.target)
            .bind(claim.organization.0)
            .bind(saved.identity)
            .bind(person)
            .bind(b.get::<Uuid, _>("executor_user_id"))
            .bind(claim.bundle)
            .bind(claim.plan)
            .bind(manifest)
            .bind(saved.source.created)
            .bind(time_basis)
            .bind(unit.get::<i64, _>("position"))
            .execute(&mut *conn)
            .await?;
    }
    let (initial_capture, semantic, created) = saved
        .baseline
        .as_ref()
        .map(|b| (b.capture, b.semantic.as_slice(), b.created))
        .unwrap_or((
            capture,
            saved.source.semantic_hmac.as_slice(),
            saved.source.created,
        ));
    sqlx::query("INSERT INTO migration_family_refresh_history_head(organization_id,identity_id,person_id,family,original_fact_id,plan_id,bundle_id,capture_id,semantic_hmac,source_created_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10 WHERE NOT EXISTS(SELECT 1 FROM migration_family_refresh_history_head WHERE organization_id=$1 AND identity_id=$2)")
        .bind(claim.organization.0).bind(saved.identity).bind(person).bind(family).bind(saved.target).bind(claim.plan).bind(claim.bundle).bind(initial_capture).bind(semantic).bind(created).execute(&mut *conn).await?;
    if saved.change == HistoryChange::Correction {
        let baseline = saved.baseline.as_ref().ok_or(MigrationError::Crypto)?;
        let version = saved.version_id.ok_or(MigrationError::Crypto)?;
        let number = baseline
            .version
            .checked_add(1)
            .ok_or(MigrationError::Crypto)?;
        let display = saved.source.display.seal(scope, key, version)?;
        sqlx::query("INSERT INTO migration_family_refresh_history_display(id,organization_id,identity_id,plan_id,bundle_id,result_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(version).bind(claim.organization.0).bind(saved.identity).bind(claim.plan).bind(claim.bundle).bind(result).bind(display.nonce.as_slice()).bind(display.ciphertext).execute(&mut *conn).await?;
        let sql=format!("INSERT INTO fub_{stem}_record_corrected(id,organization_id,identity_id,original_fact_id,person_id,actor_kind,actor_user_id,origin,occurred_at,correlation_id,version,corrects_id,plan_id,bundle_id,result_id,manifest_id,capture_id,semantic_hmac,source_created_at,source_time_basis) VALUES($1,$2,$3,$4,$5,'user',$6,'migration',clock_timestamp(),$7,$8,$9,$10,$7,$11,$12,$13,$14,$15,$16)");
        sqlx::query(&sql)
            .bind(version)
            .bind(claim.organization.0)
            .bind(saved.identity)
            .bind(saved.target)
            .bind(person)
            .bind(b.get::<Uuid, _>("executor_user_id"))
            .bind(claim.bundle)
            .bind(number)
            .bind(baseline.version_id)
            .bind(claim.plan)
            .bind(result)
            .bind(manifest)
            .bind(capture)
            .bind(saved.source.semantic_hmac.as_slice())
            .bind(saved.source.created)
            .bind(time_basis)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

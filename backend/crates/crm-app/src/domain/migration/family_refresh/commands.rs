//! Typed, retained-only bundle preparation. HTTP exposure and release admission
//! are wired with the complete workflow; this command never confirms native work.
use super::{
    evidence::{Purpose, Scope},
    model::{Family, ENGINE},
};
use crate::{
    auth::workspace::{self, ReleaseReadiness},
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{
            activity_source, core_change_store, crypto, history_capture_store,
            snapshot::SnapshotPolicy, store, MigrationError,
        },
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareFamilyRefresh {
    pub request_id: Uuid,
    pub parent_import_id: Uuid,
    pub core_report_id: Option<Uuid>,
    pub history_capture_id: Option<Uuid>,
    pub families: Vec<Family>,
}
impl PrepareFamilyRefresh {
    fn families(&self) -> Result<BTreeSet<Family>, MigrationError> {
        let families: BTreeSet<_> = self.families.iter().copied().collect();
        if families.is_empty()
            || families.len() != self.families.len()
            || families.contains(&Family::History) != self.history_capture_id.is_some()
            || families.iter().any(|f| *f != Family::History) != self.core_report_id.is_some()
        {
            return Err(MigrationError::InvalidInput);
        }
        Ok(families)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedPlan {
    pub family: Family,
    pub plan_id: Uuid,
    pub revision: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedBundle {
    pub bundle_id: Uuid,
    pub revision: String,
    pub state: String,
    pub families: Vec<PreparedPlan>,
}

pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: &ReleaseReadiness,
    ctx: &CommandContext,
    cmd: PrepareFamilyRefresh,
) -> Result<PreparedBundle, MigrationError> {
    prepare_with_readiness(pool, key, policy, Some(release), ctx, cmd).await
}

pub async fn prepare_with_readiness(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    cmd: PrepareFamilyRefresh,
) -> Result<PreparedBundle, MigrationError> {
    let families = cmd.families()?;
    let digest = crypto::request_digest(
        key,
        "family-refresh-request-v1",
        &serde_json::to_vec(&(ctx.organization_id.0, ctx.actor_user_id.0, "prepare", &cmd))
            .map_err(|_| MigrationError::Crypto)?,
    );
    let mut tx = pool.begin().await?;
    workspace::bounded_lock_wait(&mut tx).await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('crm.family_refresh_reader',$1,true)")
        .bind(ENGINE)
        .execute(&mut *tx)
        .await?;
    // Catalog handover drains compatible old writers before any retention locks.
    // Use the same order for all families so mixed requests cannot invert it.
    workspace::exclusive(&mut tx, ctx.organization_id).await?;
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    // Serialize receipts and competing preparations before looking up replay.
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let receipt = sqlx::query("SELECT r.*,p.family,p.revision,b.parent_import_id,b.parent_plan_id FROM migration_family_refresh_receipt r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id JOIN migration_family_refresh_bundle b ON b.id=r.bundle_id AND b.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.actor_user_id=$2 AND r.action='prepare' AND r.request_id=$3")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_optional(&mut *tx).await?;
    if let Some(r) = receipt {
        if r.get::<Vec<u8>, _>("input_digest") != digest {
            return Err(MigrationError::Conflict);
        }
        let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_workspace WHERE organization_id=$1 AND import_id=$2 AND plan_id=$3)")
            .bind(ctx.organization_id.0).bind(r.get::<Uuid,_>("parent_import_id")).bind(r.get::<Uuid,_>("parent_plan_id")).fetch_one(&mut *tx).await?;
        if !bound {
            return Err(MigrationError::NotFound);
        }
        let family: Family = serde_json::from_value(json!(r.get::<String, _>("family")))
            .map_err(|_| MigrationError::Crypto)?;
        let scope = Scope {
            organization: ctx.organization_id,
            bundle: r.get("bundle_id"),
            plan: r.get("plan_id"),
            family,
            revision: r.get("revision"),
        };
        return scope.open(
            key,
            cmd.request_id,
            Purpose::Receipt,
            r.get("nonce"),
            r.get("ciphertext"),
        );
    }
    let parent =
        history_capture_store::parent(&mut tx, ctx.organization_id, cmd.parent_import_id).await?;
    let parent_plan = parent
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .ok_or(MigrationError::SourceNotEligible)?;
    let account: i64 = parent.get("source_account_id");
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_bundle WHERE organization_id=$1 AND parent_import_id=$2 AND state IN ('preparing','ready','queued','running','paused'))")
        .bind(ctx.organization_id.0).bind(cmd.parent_import_id).fetch_one(&mut *tx).await?;
    if active {
        return Err(MigrationError::Conflict);
    }
    let mut core_snapshot = None;
    let mut core_inputs = Value::Null;
    let mut core_end = None;
    if let Some(report_id) = cmd.core_report_id {
        let r=sqlx::query("SELECT * FROM migration_core_change_report WHERE id=$1 AND organization_id=$2 AND state='completed'")
            .bind(report_id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::SourceNotEligible)?;
        if r.get::<Uuid, _>("parent_import_id") != cmd.parent_import_id
            || r.get::<Uuid, _>("parent_plan_id") != parent_plan
            || r.get::<i64, _>("source_account_id") != account
        {
            return Err(MigrationError::SourceNotEligible);
        }
        core_inputs = core_change_store::validate(&mut tx, key, ctx.organization_id, &r).await?;
        if core_inputs["source_scope"] != "consistent_identity" {
            return Err(MigrationError::SourceNotEligible);
        }
        core_snapshot = Some(r.get::<Uuid, _>("newer_snapshot_id"));
        core_end = Some(
            serde_json::from_value::<DateTime<Utc>>(core_inputs["newer"]["completed_at"].clone())
                .map_err(|_| MigrationError::Crypto)?,
        );
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    if core_end.is_some_and(|end| end > now) {
        return Err(MigrationError::SourceNotEligible);
    }
    let mut history_inputs = Value::Null;
    if let Some(capture) = cmd.history_capture_id {
        let h = sqlx::query(
            "SELECT * FROM migration_history_capture_run WHERE id=$1 AND organization_id=$2",
        )
        .bind(capture)
        .bind(ctx.organization_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::SourceNotEligible)?;
        let started = h
            .get::<Option<DateTime<Utc>>, _>("started_at")
            .ok_or(MigrationError::SourceNotEligible)?;
        let completed = h
            .get::<Option<DateTime<Utc>>, _>("completed_at")
            .ok_or(MigrationError::SourceNotEligible)?;
        if !history_capture_store::current_profile(&h)
            || h.get::<String, _>("state") != "completed_with_gaps"
            || h.get::<Uuid, _>("parent_import_id") != cmd.parent_import_id
            || h.get::<Uuid, _>("parent_plan_id") != parent_plan
            || h.get::<i64, _>("source_account_id") != account
            || h.get::<Option<i64>, _>("parent_source_user_id") != Some(h.get("source_user_id"))
            || completed > now
            || completed < started
            || core_end.is_some_and(|end| started <= end)
        {
            return Err(MigrationError::SourceNotEligible);
        }
        history_capture_store::verify_parent(&mut tx, ctx.organization_id, &h).await?;
        history_inputs = json!({"capture_id":capture,"capture_sequence":h.get::<i64,_>("capture_sequence").to_string(),"revision":h.get::<i64,_>("revision").to_string(),"started_at":started,"completed_at":completed,
            "profile_version":h.get::<String,_>("profile_version"),"parser_version":h.get::<String,_>("parser_version"),"schema_version":h.get::<String,_>("schema_version"),"source_user_id":h.get::<i64,_>("source_user_id").to_string(),"source_user_evidence_revision":h.get::<i32,_>("source_user_evidence_revision").to_string()});
    }
    if families.contains(&Family::Metadata) {
        release
            .ok_or(MigrationError::ReleaseNotReady)?
            .require_admitted_metadata(&mut tx)
            .await?;
        super::super::admitted_metadata::handover_qualified(&mut tx, key, ctx).await?;
    }
    let bundle = Uuid::new_v4();
    let plans: Vec<_> = families
        .into_iter()
        .map(|family| PreparedPlan {
            family,
            plan_id: Uuid::new_v4(),
            revision: "1".into(),
        })
        .collect();
    let payer = &plans[0]; // enum order selects a core family whenever one exists.
    let scope = Scope {
        organization: ctx.organization_id,
        bundle,
        plan: payer.plan_id,
        family: payer.family,
        revision: 1,
    };
    let binding = json!({"version":ENGINE,"parent_import_id":cmd.parent_import_id,"parent_plan_id":parent_plan,"account":account.to_string(),"core_report_id":cmd.core_report_id,"core":core_inputs,"history":history_inputs,
        "activity_engine":activity_source::ENGINE,"html_profile":activity_source::HTML_PROFILE,"time_profile":activity_source::TIME_PROFILE,"tzdb":activity_source::TZDB_VERSION});
    let sealed = scope.seal(key, bundle, Purpose::Binding, &binding)?;
    sqlx::query("INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,core_report_id,core_snapshot_id,history_capture_id,state,source_nonce,source_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'preparing',$11,$12)")
        .bind(bundle).bind(ctx.organization_id.0).bind(cmd.parent_import_id).bind(parent_plan).bind(account).bind(ctx.actor_user_id.0).bind(ENGINE).bind(cmd.core_report_id).bind(core_snapshot).bind(cmd.history_capture_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    for plan in &plans {
        let own = Scope {
            plan: plan.plan_id,
            family: plan.family,
            ..scope
        };
        let sealed = own.seal(key, plan.plan_id, Purpose::Binding, &binding)?;
        sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,history_capture_id,nonce,ciphertext) VALUES($1,$2,$3,$4,1,'preparing','cohort',$5,$6,$7,$8)")
            .bind(plan.plan_id).bind(bundle).bind(ctx.organization_id.0).bind(plan.family.as_str()).bind(if plan.family==Family::History {None}else{core_snapshot}).bind(if plan.family==Family::History {cmd.history_capture_id}else{None}).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE migration_family_refresh_bundle SET payer_plan_id=$2 WHERE id=$1 AND organization_id=$3").bind(bundle).bind(payer.plan_id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let payer_plan = payer.plan_id;
    let response = PreparedBundle {
        bundle_id: bundle,
        revision: "1".into(),
        state: "preparing".into(),
        families: plans,
    };
    let sealed = scope.seal(key, cmd.request_id, Purpose::Receipt, &response)?;
    sqlx::query("INSERT INTO migration_family_refresh_receipt(organization_id,actor_user_id,action,request_id,bundle_id,plan_id,input_digest,nonce,ciphertext) VALUES($1,$2,'prepare',$3,$4,$5,$6,$7,$8)")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).bind(bundle).bind(payer_plan).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    for plan in &response.families {
        let pending:i64=sqlx::query_scalar("SELECT p.measured_bytes-p.retained_bytes+CASE WHEN b.payer_plan_id=p.id THEN b.shared_measured_bytes-b.shared_retained_bytes ELSE 0 END FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2")
            .bind(plan.plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let amount = pending
            .checked_add(8192)
            .ok_or(MigrationError::StorageLimit)?;
        let token = Uuid::new_v4();
        let reserved: bool = sqlx::query_scalar(
            "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,$5,'control',$6,$7)",
        )
        .bind(ctx.organization_id.0)
        .bind(bundle)
        .bind(plan.plan_id)
        .bind(token)
        .bind(amount)
        .bind(policy.run_ceiling_bytes)
        .bind(policy.org_ceiling_bytes)
        .fetch_one(&mut *tx)
        .await?;
        if !reserved {
            return Err(MigrationError::StorageLimit);
        }
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,0,false)")
            .bind(ctx.organization_id.0)
            .bind(bundle)
            .bind(plan.plan_id)
            .bind(token)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(organization_id=%ctx.organization_id,bundle_id=%bundle,"Family refresh preparation admitted");
    Ok(response)
}

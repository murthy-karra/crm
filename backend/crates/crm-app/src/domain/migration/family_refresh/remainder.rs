//! One exact successor for a cancelled attempt. All source and mapping evidence
//! comes from a fixed confirmed plan; no source discovery runs on this path.
use super::{
    commands::{PreparedBundle, PreparedPlan},
    evidence::{Purpose, Scope},
    lifecycle::FamilyControl,
    model::{Family, ENGINE},
};
use crate::{
    auth::workspace::{self, ReleaseReadiness},
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{crypto, snapshot::SnapshotPolicy, store, MigrationError},
    },
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use uuid::Uuid;

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,predecessor_id=%predecessor))]
pub async fn create(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    predecessor: Uuid,
    cmd: FamilyControl,
) -> Result<PreparedBundle, MigrationError> {
    let expected = super::confirmation::revision(&cmd.expected_revision)?;
    let families: BTreeSet<Family> = cmd.families.iter().copied().collect();
    if families.is_empty() || families.len() != cmd.families.len() {
        return Err(MigrationError::InvalidInput);
    }
    let digest = crypto::request_digest(
        key,
        "family-refresh-request-v1",
        &serde_json::to_vec(&(
            ctx.organization_id.0,
            ctx.actor_user_id.0,
            "remainder",
            predecessor,
            &cmd,
        ))
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
    workspace::exclusive(&mut tx, ctx.organization_id).await?;
    store::require_admin(&mut tx, ctx).await?;
    store::lock_org(&mut tx, ctx.organization_id).await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let old=sqlx::query("SELECT b.* FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR UPDATE OF b").bind(predecessor).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if let Some(r)=sqlx::query("SELECT r.*,p.family,p.revision,b.predecessor_id FROM migration_family_refresh_receipt r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id JOIN migration_family_refresh_bundle b ON b.id=r.bundle_id AND b.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.actor_user_id=$2 AND r.action='remainder' AND r.request_id=$3").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_optional(&mut *tx).await? {
        if r.get::<Vec<u8>,_>("input_digest")!=digest || r.get::<Option<Uuid>,_>("predecessor_id")!=Some(predecessor){return Err(MigrationError::Conflict);}
        let scope=Scope{organization:ctx.organization_id,bundle:r.get("bundle_id"),plan:r.get("plan_id"),family:serde_json::from_value(json!(r.get::<String,_>("family"))).map_err(|_|MigrationError::Crypto)?,revision:r.get("revision")};
        return scope.open(key,cmd.request_id,Purpose::Receipt,r.get("nonce"),r.get("ciphertext"));
    }
    if old.get::<String, _>("state") != "cancelled" || old.get::<i64, _>("revision") != expected {
        return Err(MigrationError::Conflict);
    }
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_bundle WHERE organization_id=$1 AND (predecessor_id=$2 OR (parent_import_id=$3 AND state IN ('preparing','ready','queued','running','paused'))))").bind(ctx.organization_id.0).bind(predecessor).bind(old.get::<Uuid,_>("parent_import_id")).fetch_one(&mut *tx).await?;
    if exists {
        return Err(MigrationError::Conflict);
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_family_refresh(&mut tx)
        .await?;
    let mut sources = Vec::new();
    for family in &families {
        let selected=sqlx::query("SELECT * FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND family=$3 AND state<>'superseded'").bind(predecessor).bind(ctx.organization_id.0).bind(family.as_str()).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
        if selected.get::<String, _>("state") != "cancelled" {
            return Err(MigrationError::Conflict);
        }
        // Cancelling an unfinished copy does not consume the original tail.
        let source = if selected
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
            .is_some()
        {
            selected
        } else {
            let id = selected
                .get::<Option<Uuid>, _>("remainder_source_plan_id")
                .ok_or(MigrationError::Conflict)?;
            sqlx::query(
                "SELECT * FROM migration_family_refresh_plan WHERE id=$1 AND organization_id=$2",
            )
            .bind(id)
            .bind(ctx.organization_id.0)
            .fetch_one(&mut *tx)
            .await?
        };
        if source
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
            .is_none()
            || !matches!(
                source.get::<String, _>("state").as_str(),
                "cancelled" | "completed" | "superseded"
            )
        {
            return Err(MigrationError::Conflict);
        }
        let unfinished:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_manifest u WHERE u.plan_id=$1 AND u.organization_id=$2 AND u.position>$3 AND u.kind<>'catalog' AND u.disposition IN ('insert','update','already_current','correction') AND NOT EXISTS(SELECT 1 FROM migration_family_refresh_result r WHERE r.manifest_id=u.id AND r.organization_id=u.organization_id))").bind(source.get::<Uuid,_>("id")).bind(ctx.organization_id.0).bind(source.get::<i64,_>("apply_position")).fetch_one(&mut *tx).await?;
        if !unfinished {
            return Err(MigrationError::Conflict);
        }
        sources.push((*family, source));
    }
    let source_bundle: Uuid = sources[0].1.get("bundle_id");
    let origin = old
        .get::<Option<Uuid>, _>("remainder_origin_bundle_id")
        .unwrap_or(predecessor);
    for (_, plan) in &sources {
        let compatible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2 AND COALESCE(remainder_origin_bundle_id,id)=$3 AND parent_import_id=$4 AND parent_plan_id=$5 AND source_account_id=$6 AND state='cancelled')")
            .bind(plan.get::<Uuid,_>("bundle_id")).bind(ctx.organization_id.0).bind(origin).bind(old.get::<Uuid,_>("parent_import_id")).bind(old.get::<Uuid,_>("parent_plan_id")).bind(old.get::<i64,_>("source_account_id")).fetch_one(&mut *tx).await?;
        if !compatible {
            return Err(MigrationError::Crypto);
        }
    }
    let source=sqlx::query("SELECT b.*,p.family AS payer_family,p.revision AS payer_revision FROM migration_family_refresh_bundle b JOIN migration_family_refresh_plan p ON p.id=b.payer_plan_id AND p.organization_id=b.organization_id WHERE b.id=$1 AND b.organization_id=$2 AND b.state='cancelled'").bind(source_bundle).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Conflict)?;
    for name in ["parent_import_id", "parent_plan_id"] {
        if source.get::<Uuid, _>(name) != old.get::<Uuid, _>(name) {
            return Err(MigrationError::Crypto);
        }
    }
    if source.get::<i64, _>("source_account_id") != old.get::<i64, _>("source_account_id") {
        return Err(MigrationError::Crypto);
    }
    let source_scope = Scope {
        organization: ctx.organization_id,
        bundle: source_bundle,
        plan: source.get("payer_plan_id"),
        family: serde_json::from_value(json!(source.get::<String, _>("payer_family")))
            .map_err(|_| MigrationError::Crypto)?,
        revision: source.get("payer_revision"),
    };
    let binding: Value = source_scope.open(
        key,
        source_bundle,
        Purpose::Binding,
        source.get("source_nonce"),
        source.get("source_ciphertext"),
    )?;
    super::plan_commands::validate_runtime(&binding)?;
    let id = Uuid::new_v4();
    let payer = Uuid::new_v4();
    let scope = Scope {
        organization: ctx.organization_id,
        bundle: id,
        plan: payer,
        family: *families.first().ok_or(MigrationError::InvalidInput)?,
        revision: 1,
    };
    let sealed = scope.seal(key, id, Purpose::Binding, &binding)?;
    let origin = old
        .get::<Option<Uuid>, _>("remainder_origin_bundle_id")
        .unwrap_or(predecessor);
    sqlx::query("INSERT INTO migration_family_refresh_bundle(id,organization_id,parent_import_id,parent_plan_id,source_account_id,executor_user_id,engine_version,core_report_id,core_snapshot_id,history_capture_id,state,source_nonce,source_ciphertext,predecessor_id,remainder_source_bundle_id,remainder_origin_bundle_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'preparing',$11,$12,$13,$14,$15)")
        .bind(id).bind(ctx.organization_id.0).bind(source.get::<Uuid,_>("parent_import_id")).bind(source.get::<Uuid,_>("parent_plan_id")).bind(source.get::<i64,_>("source_account_id")).bind(ctx.actor_user_id.0).bind(ENGINE).bind(source.get::<Option<Uuid>,_>("core_report_id")).bind(source.get::<Option<Uuid>,_>("core_snapshot_id")).bind(source.get::<Option<Uuid>,_>("history_capture_id")).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(predecessor).bind(source_bundle).bind(origin).execute(&mut *tx).await?;
    let mut plans = Vec::new();
    for (index, (family, old_plan)) in sources.iter().enumerate() {
        let plan = if index == 0 { payer } else { Uuid::new_v4() };
        let own = Scope {
            plan,
            family: *family,
            ..scope
        };
        let old_scope = Scope {
            bundle: old_plan.get("bundle_id"),
            plan: old_plan.get("id"),
            family: *family,
            revision: old_plan.get("revision"),
            ..source_scope
        };
        let plan_binding: Value = old_scope.open(
            key,
            old_scope.plan,
            Purpose::Binding,
            old_plan.get("nonce"),
            old_plan.get("ciphertext"),
        )?;
        if plan_binding != binding {
            return Err(MigrationError::Crypto);
        }
        let sealed = own.seal(key, plan, Purpose::Binding, &plan_binding)?;
        sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,history_capture_id,nonce,ciphertext,remainder_source_plan_id,remainder_complete) VALUES($1,$2,$3,$4,1,'preparing','cohort',$5,$6,$7,$8,$9,false)")
            .bind(plan).bind(id).bind(ctx.organization_id.0).bind(family.as_str()).bind(old_plan.get::<Option<Uuid>,_>("source_snapshot_id")).bind(old_plan.get::<Option<Uuid>,_>("history_capture_id")).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(old_plan.get::<Uuid,_>("id")).execute(&mut *tx).await?;
        plans.push(PreparedPlan {
            family: *family,
            plan_id: plan,
            revision: "1".into(),
        });
    }
    sqlx::query("UPDATE migration_family_refresh_bundle SET payer_plan_id=$2 WHERE id=$1 AND organization_id=$3").bind(id).bind(payer).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    let response = PreparedBundle {
        bundle_id: id,
        revision: "1".into(),
        state: "preparing".into(),
        families: plans,
    };
    let sealed = scope.seal(key, cmd.request_id, Purpose::Receipt, &response)?;
    sqlx::query("INSERT INTO migration_family_refresh_receipt(organization_id,actor_user_id,action,request_id,bundle_id,plan_id,input_digest,nonce,ciphertext) VALUES($1,$2,'remainder',$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).bind(id).bind(payer).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    for plan in &response.families {
        let pending:i64=sqlx::query_scalar("SELECT p.measured_bytes-p.retained_bytes+CASE WHEN b.payer_plan_id=p.id THEN b.shared_measured_bytes-b.shared_retained_bytes ELSE 0 END FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE p.id=$1 AND p.organization_id=$2").bind(plan.plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let token = Uuid::new_v4();
        let amount = pending
            .checked_add(8192)
            .ok_or(MigrationError::StorageLimit)?;
        let reserved: bool = sqlx::query_scalar(
            "SELECT crm_family_refresh_reserve($1,$2,$3,$4,0,$5,'control',$6,$7)",
        )
        .bind(ctx.organization_id.0)
        .bind(id)
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
            .bind(id)
            .bind(plan.plan_id)
            .bind(token)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(response)
}

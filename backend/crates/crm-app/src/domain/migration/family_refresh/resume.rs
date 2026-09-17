//! Explicit administrator adoption of paused, frozen family work.
use super::{
    commands::{PreparedBundle, PreparedPlan},
    confirmation::revision,
    evidence::{Purpose, Scope},
    lifecycle::FamilyControl,
    sealing,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{crypto, snapshot::SnapshotPolicy, MigrationError},
    },
};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use uuid::Uuid;

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%id))]
pub async fn resume(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: &ReleaseReadiness,
    ctx: &CommandContext,
    id: Uuid,
    cmd: FamilyControl,
) -> Result<PreparedBundle, MigrationError> {
    resume_with_readiness(pool, key, policy, Some(release), ctx, id, cmd).await
}

pub async fn resume_with_readiness(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    id: Uuid,
    cmd: FamilyControl,
) -> Result<PreparedBundle, MigrationError> {
    let expected = revision(&cmd.expected_revision)?;
    let selected: BTreeSet<_> = cmd.families.iter().copied().collect();
    if selected.is_empty() || selected.len() != cmd.families.len() {
        return Err(MigrationError::InvalidInput);
    }
    let digest = crypto::request_digest(
        key,
        "family-refresh-request-v1",
        &serde_json::to_vec(&(
            ctx.organization_id.0,
            ctx.actor_user_id.0,
            "resume",
            id,
            &cmd,
        ))
        .map_err(|_| MigrationError::Crypto)?,
    );
    let mut tx = super::queries::begin(pool, ctx).await?;
    sqlx::query("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF m").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let b=sqlx::query("SELECT b.* FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR UPDATE OF b").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if let Some(r)=sqlx::query("SELECT r.*,p.family,p.revision FROM migration_family_refresh_receipt r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.actor_user_id=$2 AND r.action='resume' AND r.request_id=$3").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_optional(&mut *tx).await? {
        if r.get::<Vec<u8>,_>("input_digest")!=digest || r.get::<Uuid,_>("bundle_id")!=id {return Err(MigrationError::Conflict);}
        let scope=Scope{organization:ctx.organization_id,bundle:id,plan:r.get("plan_id"),family:serde_json::from_value(serde_json::json!(r.get::<String,_>("family"))).map_err(|_|MigrationError::Crypto)?,revision:r.get("revision")};
        return scope.open(key,cmd.request_id,Purpose::Receipt,r.get("nonce"),r.get("ciphertext"));
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_family_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    if b.get::<i64, _>("revision") != expected
        || matches!(
            b.get::<String, _>("state").as_str(),
            "cancelled" | "completed"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let plans=sqlx::query("SELECT * FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state<>'superseded' ORDER BY family FOR UPDATE").bind(id).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    if selected.iter().any(|f| {
        !plans.iter().any(|p| {
            p.get::<String, _>("family") == f.as_str() && p.get::<String, _>("state") == "paused"
        })
    }) {
        return Err(MigrationError::Conflict);
    }
    let confirmed = b
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
        .is_some();
    let next = expected.checked_add(1).ok_or(MigrationError::Conflict)?;
    let state = if confirmed { "queued" } else { "preparing" };
    let adopted = b.get::<Uuid, _>("executor_user_id") != ctx.actor_user_id.0;
    sqlx::query("SELECT set_config('crm.family_refresh_adopt_executor',$1,true)")
        .bind(ctx.actor_user_id.0.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE migration_family_refresh_bundle SET executor_user_id=$3,revision=$4,state=$5,digest=CASE WHEN confirmed_at IS NULL THEN NULL ELSE digest END,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(next).bind(state).execute(&mut *tx).await?;
    let mut affected = BTreeSet::new();
    let mut resumed = Vec::new();
    for p in &plans {
        let plan: Uuid = p.get("id");
        let family = selected
            .iter()
            .find(|f| f.as_str() == p.get::<String, _>("family"));
        if let Some(family) = family {
            sqlx::query("UPDATE migration_family_refresh_plan SET state=$3,pause_reason=NULL,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).bind(state).execute(&mut *tx).await?;
            resumed.push(PreparedPlan {
                family: *family,
                plan_id: plan,
                revision: p.get::<i64, _>("revision").to_string(),
            });
            affected.insert(plan);
        } else if adopted
            && matches!(
                p.get::<String, _>("state").as_str(),
                "preparing" | "ready" | "queued" | "running"
            )
        {
            // Adoption must not silently restart a family omitted by this request.
            sqlx::query("UPDATE migration_family_refresh_plan SET state='paused',pause_reason='executor_replaced',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).execute(&mut *tx).await?;
            affected.insert(plan);
        }
    }
    affected.insert(b.get("payer_plan_id"));
    // Replenish control capacity before adding another receipt. Failure rolls
    // back adoption and all state changes, preserving the cancellation allowance.
    for plan in &affected {
        let r=sqlx::query("SELECT r.token,p.lease_epoch FROM migration_family_refresh_reservation r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.organization_id=r.organization_id WHERE r.plan_id=$1 AND r.organization_id=$2 AND r.purpose='control'").bind(plan).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let epoch: i64 = r.get("lease_epoch");
        sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,true)")
            .bind(ctx.organization_id.0)
            .bind(id)
            .bind(plan)
            .bind(r.get::<Uuid, _>("token"))
            .bind(epoch)
            .execute(&mut *tx)
            .await?;
        let reserved: bool = sqlx::query_scalar(
            "SELECT crm_family_refresh_reserve($1,$2,$3,$4,$5,8192,'control',$6,$7)",
        )
        .bind(ctx.organization_id.0)
        .bind(id)
        .bind(plan)
        .bind(Uuid::new_v4())
        .bind(epoch)
        .bind(policy.run_ceiling_bytes)
        .bind(policy.org_ceiling_bytes)
        .fetch_one(&mut *tx)
        .await?;
        if !reserved {
            return Err(MigrationError::StorageLimit);
        }
    }
    let response = PreparedBundle {
        bundle_id: id,
        revision: next.to_string(),
        state: state.into(),
        families: resumed,
    };
    let owner = &response.families[0];
    let scope = Scope {
        organization: ctx.organization_id,
        bundle: id,
        plan: owner.plan_id,
        family: owner.family,
        revision: revision(&owner.revision)?,
    };
    let sealed = scope.seal(key, cmd.request_id, Purpose::Receipt, &response)?;
    sqlx::query("INSERT INTO migration_family_refresh_receipt(organization_id,actor_user_id,action,request_id,bundle_id,plan_id,input_digest,nonce,ciphertext) VALUES($1,$2,'resume',$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).bind(id).bind(owner.plan_id).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    for plan in affected {
        sealing::settle_control(&mut tx, ctx.organization_id.0, id, plan).await?;
    }
    tx.commit().await?;
    Ok(response)
}

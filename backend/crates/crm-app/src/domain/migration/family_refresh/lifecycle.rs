//! Scoped, replayable control commands; no control command executes native units.
use super::{
    commands::{PreparedBundle, PreparedPlan},
    confirmation::revision,
    evidence::{Purpose, Scope},
    model::Family,
    sealing,
};
use crate::{
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{crypto, MigrationError},
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyControl {
    pub request_id: Uuid,
    pub expected_revision: String,
    pub families: Vec<Family>,
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%id))]
pub async fn cancel(
    pool: &PgPool,
    key: &RawPayloadKey,
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
            "cancel",
            id,
            &cmd,
        ))
        .map_err(|_| MigrationError::Crypto)?,
    );
    let mut tx = super::queries::begin(pool, ctx).await?;
    sqlx::query("SELECT set_config('crm.family_refresh_cancel_actor',$1,true),set_config('crm.family_refresh_cancel_request',$2,true)").bind(ctx.actor_user_id.0.to_string()).bind(cmd.request_id.to_string()).execute(&mut *tx).await?;
    // Keep the executor membership stable for control settlement. Cancellation
    // does not silently adopt the requesting administrator as executor.
    sqlx::query("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 FOR SHARE OF m").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let b=sqlx::query("SELECT b.* FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR UPDATE OF b").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if let Some(r)=sqlx::query("SELECT r.*,p.family,p.revision FROM migration_family_refresh_receipt r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.actor_user_id=$2 AND r.action='cancel' AND r.request_id=$3").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_optional(&mut *tx).await? {
        if r.get::<Vec<u8>,_>("input_digest")!=digest || r.get::<Uuid,_>("bundle_id")!=id {return Err(MigrationError::Conflict);}
        let scope=Scope{organization:ctx.organization_id,bundle:id,plan:r.get("plan_id"),family:serde_json::from_value(serde_json::json!(r.get::<String,_>("family"))).map_err(|_|MigrationError::Crypto)?,revision:r.get("revision")};
        return scope.open(key,cmd.request_id,Purpose::Receipt,r.get("nonce"),r.get("ciphertext"));
    }
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
        !plans
            .iter()
            .any(|p| p.get::<String, _>("family") == f.as_str())
    }) {
        return Err(MigrationError::NotFound);
    }
    let survives = plans.iter().any(|p| {
        !selected
            .iter()
            .any(|f| f.as_str() == p.get::<String, _>("family"))
            && !matches!(
                p.get::<String, _>("state").as_str(),
                "cancelled" | "completed"
            )
    });
    let mut changed = Vec::new();
    for p in &plans {
        let Some(family) = selected
            .iter()
            .find(|f| f.as_str() == p.get::<String, _>("family"))
        else {
            continue;
        };
        if matches!(
            p.get::<String, _>("state").as_str(),
            "cancelled" | "completed"
        ) || p.get::<bool, _>("cancel_requested")
        {
            return Err(MigrationError::Conflict);
        }
        let plan: Uuid = p.get("id");
        let shared = survives
            && plan == b.get::<Uuid, _>("payer_plan_id")
            && matches!(p.get::<String, _>("phase").as_str(), "cohort" | "capture");
        sqlx::query("UPDATE migration_family_refresh_plan SET cancel_requested=true,state=CASE WHEN $3 THEN state ELSE 'cancelled' END,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).bind(shared).execute(&mut *tx).await?;
        changed.push(PreparedPlan {
            family: *family,
            plan_id: plan,
            revision: p.get::<i64, _>("revision").to_string(),
        });
    }
    let next = expected.checked_add(1).ok_or(MigrationError::Conflict)?;
    let state = if survives {
        b.get::<String, _>("state")
    } else {
        "cancelled".into()
    };
    sqlx::query("UPDATE migration_family_refresh_bundle SET state=$3,revision=$4,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(&state).bind(next).execute(&mut *tx).await?;
    // Before confirmation a cancellation invalidates the old combined digest;
    // selected ready siblings can be confirmed against this exact new revision.
    if b.get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
        .is_none()
    {
        let hash = sealing::bundle_digest(&mut tx, key, ctx.organization_id.0, id).await?;
        sqlx::query("UPDATE migration_family_refresh_bundle SET digest=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(hash).execute(&mut *tx).await?;
    }
    let response = PreparedBundle {
        bundle_id: id,
        revision: next.to_string(),
        state,
        families: changed,
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
    sqlx::query("INSERT INTO migration_family_refresh_receipt(organization_id,actor_user_id,action,request_id,bundle_id,plan_id,input_digest,nonce,ciphertext) VALUES($1,$2,'cancel',$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).bind(id).bind(owner.plan_id).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    let mut payers: BTreeSet<_> = response.families.iter().map(|p| p.plan_id).collect();
    payers.insert(b.get("payer_plan_id"));
    for plan in payers {
        sealing::settle_control(&mut tx, ctx.organization_id.0, id, plan).await?;
    }
    tx.commit().await?;
    Ok(response)
}

/// Finish a requested cancellation once the fixed payer has completed shared
/// work. This bounded turn does not authenticate or classify its own native units.
pub async fn finish_cancel(pool: &PgPool) -> Result<bool, MigrationError> {
    let row=sqlx::query("SELECT p.id,p.bundle_id,p.organization_id FROM migration_family_refresh_plan p JOIN migration_family_refresh_bundle b ON b.id=p.bundle_id AND b.organization_id=p.organization_id WHERE p.cancel_requested AND p.state='preparing' AND p.phase NOT IN ('cohort','capture') AND b.state='preparing' AND EXISTS(SELECT 1 FROM organization_membership m WHERE m.organization_id=b.organization_id AND m.user_id=b.executor_user_id AND m.role='admin' AND m.status='active') ORDER BY p.created_at,p.id LIMIT 1").fetch_optional(pool).await?;
    let Some(row) = row else { return Ok(false) };
    let org = crate::ids::OrganizationId::new(row.get("organization_id"));
    let bundle: Uuid = row.get("bundle_id");
    let plan: Uuid = row.get("id");
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('crm.family_refresh_reader',$1,true)")
        .bind(super::model::ENGINE)
        .execute(&mut *tx)
        .await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    crate::auth::workspace::shared(&mut tx, org).await?;
    sqlx::query("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 AND m.role='admin' AND m.status='active' FOR SHARE OF m").bind(bundle).bind(org.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::Forbidden)?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(org.0).fetch_one(&mut *tx).await?;
    sqlx::query("SELECT id FROM migration_family_refresh_bundle WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(bundle).bind(org.0).fetch_one(&mut *tx).await?;
    let changed=sqlx::query("UPDATE migration_family_refresh_plan SET state='cancelled',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND cancel_requested AND state='preparing' AND phase NOT IN ('cohort','capture')").bind(plan).bind(org.0).execute(&mut *tx).await?.rows_affected();
    if changed == 0 {
        return Ok(false);
    }
    sealing::settle_control(&mut tx, org.0, bundle, plan).await?;
    tx.commit().await?;
    Ok(true)
}

//! Exact, replayable admission of a frozen group of family plans.
use super::{
    commands::{PreparedBundle, PreparedPlan},
    evidence::{Purpose, Scope},
    model::{Counts, Family, ENGINE},
    sealing,
};
use crate::{
    auth::workspace::ReleaseReadiness,
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{crypto, imports, MigrationError},
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedPlan {
    pub family: Family,
    pub plan_id: Uuid,
    pub plan_revision: String,
    pub plan_digest: String,
    pub expected_counts: Counts,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmFamilyRefresh {
    pub request_id: Uuid,
    pub expected_revision: String,
    pub bundle_digest: String,
    pub families: Vec<SelectedPlan>,
    pub acknowledged_exclusions: bool,
}
pub(super) fn revision(raw: &str) -> Result<i64, MigrationError> {
    let n = raw
        .parse::<i64>()
        .map_err(|_| MigrationError::InvalidInput)?;
    if n <= 0 || n.to_string() != raw {
        return Err(MigrationError::InvalidInput);
    }
    Ok(n)
}
fn valid_digest(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%id))]
pub async fn confirm(
    pool: &PgPool,
    key: &RawPayloadKey,
    release: &ReleaseReadiness,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmFamilyRefresh,
) -> Result<PreparedBundle, MigrationError> {
    confirm_with_readiness(pool, key, Some(release), ctx, id, cmd).await
}

pub async fn confirm_with_readiness(
    pool: &PgPool,
    key: &RawPayloadKey,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    id: Uuid,
    cmd: ConfirmFamilyRefresh,
) -> Result<PreparedBundle, MigrationError> {
    let expected = revision(&cmd.expected_revision)?;
    let selected: BTreeSet<_> = cmd.families.iter().map(|p| p.family).collect();
    if selected.is_empty()
        || selected.len() != cmd.families.len()
        || !valid_digest(&cmd.bundle_digest)
        || cmd.families.iter().any(|p| {
            !valid_digest(&p.plan_digest)
                || revision(&p.plan_revision).is_err()
                || !p.expected_counts.reconciles()
        })
    {
        return Err(MigrationError::InvalidInput);
    }
    let request_digest = crypto::request_digest(
        key,
        "family-refresh-request-v1",
        &serde_json::to_vec(&(
            ctx.organization_id.0,
            ctx.actor_user_id.0,
            "confirm",
            id,
            &cmd,
        ))
        .map_err(|_| MigrationError::Crypto)?,
    );
    let mut tx = super::queries::begin(pool, ctx).await?;
    let executor:Option<Uuid>=sqlx::query_scalar("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id WHERE b.id=$1 AND b.organization_id=$2 AND m.role='admin' AND m.status='active' FOR SHARE OF m").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let b=sqlx::query("SELECT b.* FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR UPDATE OF b").bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if let Some(r)=sqlx::query("SELECT r.*,p.family,p.revision FROM migration_family_refresh_receipt r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.actor_user_id=$2 AND r.action='confirm' AND r.request_id=$3").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_optional(&mut *tx).await? {
        if r.get::<Vec<u8>,_>("input_digest")!=request_digest || r.get::<Uuid,_>("bundle_id")!=id {return Err(MigrationError::Conflict);}
        let scope=Scope{organization:ctx.organization_id,bundle:id,plan:r.get("plan_id"),family:serde_json::from_value(serde_json::json!(r.get::<String,_>("family"))).map_err(|_|MigrationError::Crypto)?,revision:r.get("revision")};
        return scope.open(key,cmd.request_id,Purpose::Receipt,r.get("nonce"),r.get("ciphertext"));
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_family_refresh(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    if executor != Some(b.get::<Uuid, _>("executor_user_id"))
        || b.get::<i64, _>("revision") != expected
        || b.get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
            .is_some()
        || !matches!(b.get::<String, _>("state").as_str(), "preparing" | "ready")
    {
        return Err(MigrationError::Conflict);
    }
    let combined = sealing::bundle_digest(&mut tx, key, ctx.organization_id.0, id).await?;
    if b.get::<Option<Vec<u8>>, _>("digest").as_ref() != Some(&combined)
        || imports::hex(&combined) != cmd.bundle_digest
    {
        return Err(MigrationError::Conflict);
    }
    let plans=sqlx::query("SELECT *,expires_at>clock_timestamp() AS unexpired,lease_expires_at>clock_timestamp() AS lease_live FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state<>'superseded' ORDER BY family FOR UPDATE").bind(id).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    if plans.len() < selected.len()
        || selected.iter().any(|f| {
            !plans
                .iter()
                .any(|p| p.get::<String, _>("family") == f.as_str())
        })
    {
        return Err(MigrationError::Conflict);
    }
    let mut exclusions = plans.len() != selected.len();
    for p in &plans {
        if p.get::<Option<bool>, _>("lease_live") == Some(true) {
            return Err(MigrationError::ImportBusy);
        }
        let family = p.get::<String, _>("family");
        let Some(want) = cmd.families.iter().find(|f| f.family.as_str() == family) else {
            continue;
        };
        let counts: Counts =
            serde_json::from_value(p.get("counts")).map_err(|_| MigrationError::Crypto)?;
        if p.get::<Uuid, _>("id") != want.plan_id
            || p.get::<i64, _>("revision") != revision(&want.plan_revision)?
            || p.get::<String, _>("state") != "ready"
            || p.get::<Option<bool>, _>("unexpired") != Some(true)
            || p.get::<Option<Vec<u8>>, _>("digest")
                .map(|v| imports::hex(&v))
                .as_deref()
                != Some(&want.plan_digest)
            || counts != want.expected_counts
        {
            return Err(MigrationError::Conflict);
        }
        let useful:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_manifest WHERE plan_id=$1 AND organization_id=$2 AND kind<>'catalog' AND disposition IN ('insert','update','already_current','correction'))").bind(want.plan_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        if !useful {
            return Err(MigrationError::SourceNotEligible);
        }
        exclusions |= counts.held > 0 || counts.excluded > 0;
    }
    if exclusions && !cmd.acknowledged_exclusions {
        return Err(MigrationError::InvalidInput);
    }
    sqlx::query("INSERT INTO migration_family_refresh_requirement(organization_id,capability,bundle_id) VALUES($1,$2,$3) ON CONFLICT(organization_id) DO NOTHING").bind(ctx.organization_id.0).bind(ENGINE).bind(id).execute(&mut *tx).await?;
    for p in &plans {
        let selected = cmd
            .families
            .iter()
            .any(|f| f.plan_id == p.get::<Uuid, _>("id"));
        if selected {
            sqlx::query("UPDATE migration_family_refresh_plan SET state='queued',phase='apply',confirmed_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(p.get::<Uuid,_>("id")).bind(ctx.organization_id.0).execute(&mut *tx).await?;
        } else if p.get::<String, _>("state") != "cancelled" {
            sqlx::query("UPDATE migration_family_refresh_plan SET state='cancelled',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(p.get::<Uuid,_>("id")).bind(ctx.organization_id.0).execute(&mut *tx).await?;
        }
    }
    let next = expected.checked_add(1).ok_or(MigrationError::Conflict)?;
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='queued',revision=$3,confirmed_at=clock_timestamp(),updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(next).execute(&mut *tx).await?;
    let response = PreparedBundle {
        bundle_id: id,
        revision: next.to_string(),
        state: "queued".into(),
        families: cmd
            .families
            .iter()
            .map(|p| PreparedPlan {
                family: p.family,
                plan_id: p.plan_id,
                revision: p.plan_revision.clone(),
            })
            .collect(),
    };
    let owner = &cmd.families[0];
    let scope = Scope {
        organization: ctx.organization_id,
        bundle: id,
        plan: owner.plan_id,
        family: owner.family,
        revision: revision(&owner.plan_revision)?,
    };
    let sealed = scope.seal(key, cmd.request_id, Purpose::Receipt, &response)?;
    sqlx::query("INSERT INTO migration_family_refresh_receipt(organization_id,actor_user_id,action,request_id,bundle_id,plan_id,input_digest,nonce,ciphertext) VALUES($1,$2,'confirm',$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).bind(id).bind(owner.plan_id).bind(request_digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    let mut payers: BTreeSet<Uuid> = plans.iter().map(|p| p.get("id")).collect();
    payers.insert(b.get("payer_plan_id"));
    for plan in payers {
        sealing::settle_control(&mut tx, ctx.organization_id.0, id, plan).await?;
    }
    tx.commit().await?;
    Ok(response)
}

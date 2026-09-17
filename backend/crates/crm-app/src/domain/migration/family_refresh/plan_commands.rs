//! Explicit mapping changes create immutable successor plans. Only bounded
//! patches are admitted here; the worker rebuilds the catalog over shared source.
use super::{
    commands::{PreparedBundle, PreparedPlan},
    evidence::{Purpose, Scope},
    mapping_inventory::{Choice, Mapping},
    mapping_selection::{self, MappingPatch, Patch},
    model::{Family, ENGINE},
};
use crate::{
    config::RawPayloadKey,
    domain::{
        envelope::CommandContext,
        migration::{
            activity_source, activity_store, crypto, snapshot::SnapshotPolicy, MigrationError,
        },
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

fn zone<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanFamilyRefresh {
    pub request_id: Uuid,
    pub expected_revision: String,
    pub family: Family,
    #[serde(default)]
    pub patches: Vec<MappingPatch>,
    #[serde(
        default,
        deserialize_with = "zone",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_timezone: Option<Option<String>>,
}
fn revision(raw: &str) -> Result<i64, MigrationError> {
    let n = raw
        .parse::<i64>()
        .map_err(|_| MigrationError::InvalidInput)?;
    if n <= 0 || n.to_string() != raw {
        return Err(MigrationError::InvalidInput);
    }
    Ok(n)
}

#[tracing::instrument(skip_all,fields(organization_id=%ctx.organization_id.0,bundle_id=%id))]
pub async fn plan(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    id: Uuid,
    cmd: PlanFamilyRefresh,
) -> Result<PreparedBundle, MigrationError> {
    let expected = revision(&cmd.expected_revision)?;
    if cmd.patches.len() > 50
        || cmd
            .patches
            .iter()
            .map(|p| p.mapping_id)
            .collect::<BTreeSet<_>>()
            .len()
            != cmd.patches.len()
        || (cmd.family == Family::History && !cmd.patches.is_empty())
        || (cmd.family != Family::Activity && cmd.source_timezone.is_some())
    {
        return Err(MigrationError::InvalidInput);
    }
    let digest = crypto::request_digest(
        key,
        "family-refresh-request-v1",
        &serde_json::to_vec(&(ctx.organization_id.0, ctx.actor_user_id.0, "plan", id, &cmd))
            .map_err(|_| MigrationError::Crypto)?,
    );
    let mut tx = super::queries::begin(pool, ctx).await?;
    let executor:Option<Uuid>=sqlx::query_scalar("SELECT m.user_id FROM organization_membership m JOIN migration_family_refresh_bundle b ON b.organization_id=m.organization_id AND b.executor_user_id=m.user_id JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 AND m.role='admin' AND m.status='active' FOR SHARE OF m")
        .bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?;
    sqlx::query("SELECT organization_id FROM migration_snapshot_storage WHERE organization_id=$1 FOR UPDATE").bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let b=sqlx::query("SELECT b.* FROM migration_family_refresh_bundle b JOIN migration_workspace w ON w.organization_id=b.organization_id AND w.import_id=b.parent_import_id AND w.plan_id=b.parent_plan_id WHERE b.id=$1 AND b.organization_id=$2 FOR UPDATE OF b")
        .bind(id).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    // Authorized replay precedes current revision/state/expiry/capacity checks.
    if let Some(r)=sqlx::query("SELECT r.*,p.family,p.revision FROM migration_family_refresh_receipt r JOIN migration_family_refresh_plan p ON p.id=r.plan_id AND p.bundle_id=r.bundle_id AND p.organization_id=r.organization_id WHERE r.organization_id=$1 AND r.actor_user_id=$2 AND r.action='plan' AND r.request_id=$3")
        .bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).fetch_optional(&mut *tx).await? {
        if r.get::<Vec<u8>,_>("input_digest")!=digest || r.get::<Uuid,_>("bundle_id")!=id {return Err(MigrationError::Conflict);}
        let scope=Scope{organization:ctx.organization_id,bundle:id,plan:r.get("plan_id"),family:serde_json::from_value(serde_json::json!(r.get::<String,_>("family"))).map_err(|_|MigrationError::Crypto)?,revision:r.get("revision")};
        return scope.open(key,cmd.request_id,Purpose::Receipt,r.get("nonce"),r.get("ciphertext"));
    }
    if executor != Some(b.get::<Uuid, _>("executor_user_id"))
        || b.get::<Option<Uuid>, _>("predecessor_id").is_some()
        || b.get::<i64, _>("revision") != expected
        || b.get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
            .is_some()
        || !matches!(
            b.get::<String, _>("state").as_str(),
            "preparing" | "ready" | "paused"
        )
    {
        return Err(MigrationError::Conflict);
    }
    let old=sqlx::query("SELECT *,lease_expires_at>clock_timestamp() AS lease_live FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND family=$3 AND state<>'superseded' FOR UPDATE")
        .bind(id).bind(ctx.organization_id.0).bind(cmd.family.as_str()).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    let old_id: Uuid = old.get("id");
    if old.get::<Option<bool>, _>("lease_live") == Some(true)
        || old
            .get::<Option<chrono::DateTime<chrono::Utc>>, _>("confirmed_at")
            .is_some()
        || !matches!(
            old.get::<String, _>("state").as_str(),
            "preparing" | "ready" | "paused"
        )
        || !matches!(
            old.get::<String, _>("phase").as_str(),
            "mappings" | "classify"
        )
        || (cmd.family != Family::History && !old.get::<bool, _>("mappings_complete"))
    {
        return Err(MigrationError::ImportBusy);
    }
    if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM migration_family_refresh_reservation WHERE plan_id=$1 AND organization_id=$2 AND purpose='unit')").bind(old_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await? {return Err(MigrationError::ImportBusy);}
    let old_scope = Scope {
        organization: ctx.organization_id,
        bundle: id,
        plan: old_id,
        family: cmd.family,
        revision: old.get("revision"),
    };
    let binding: serde_json::Value = old_scope.open(
        key,
        old_id,
        Purpose::Binding,
        old.get("nonce"),
        old.get("ciphertext"),
    )?;
    let owner=sqlx::query("SELECT family,revision FROM migration_family_refresh_plan WHERE id=$1 AND bundle_id=$2 AND organization_id=$3").bind(b.get::<Uuid,_>("payer_plan_id")).bind(id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    let owner_scope = Scope {
        plan: b.get("payer_plan_id"),
        family: serde_json::from_value(serde_json::json!(owner.get::<String, _>("family")))
            .map_err(|_| MigrationError::Crypto)?,
        revision: owner.get("revision"),
        ..old_scope
    };
    let frozen: serde_json::Value = owner_scope.open(
        key,
        id,
        Purpose::Binding,
        b.get("source_nonce"),
        b.get("source_ciphertext"),
    )?;
    if frozen != binding
        || binding["parent_import_id"] != serde_json::json!(b.get::<Uuid, _>("parent_import_id"))
        || binding["parent_plan_id"] != serde_json::json!(b.get::<Uuid, _>("parent_plan_id"))
        || binding["account"]
            .as_str()
            .and_then(|v| v.parse::<i64>().ok())
            != Some(b.get::<i64, _>("source_account_id"))
    {
        return Err(MigrationError::Crypto);
    }
    validate_runtime(&binding)?;
    let patches = patches(&mut tx, key, old_scope, &cmd.patches).await?;
    let timezone = if cmd.family == Family::Activity {
        if let Some(zone) = cmd.source_timezone {
            Some(mapping_selection::timezone(zone)?)
        } else {
            let hash = activity_store::source_key(
                key,
                ctx.organization_id,
                b.get("source_account_id"),
                "timezone",
                b"source_timezone",
            );
            if let Some(r)=sqlx::query("SELECT *,NULL::bytea AS parent_key FROM migration_family_refresh_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='timezone' AND source_key_hmac=$3").bind(old_id).bind(ctx.organization_id.0).bind(hash).fetch_optional(&mut *tx).await? {
                let data:Mapping=old_scope.open(key,r.get("id"),Purpose::Mapping,r.get("nonce"),r.get("ciphertext"))?;
                data.verify(&r)?;
                if data.kind!="timezone" || data.source.is_some() || !matches!(data.choice,Choice::Hold|Choice::Timezone{..}) {return Err(MigrationError::Crypto);}
                Some(data.choice)
            }else{None}
        }
    } else {
        None
    };
    let new_id = Uuid::new_v4();
    let new_revision = old_scope
        .revision
        .checked_add(1)
        .ok_or(MigrationError::Conflict)?;
    let bundle_revision = expected.checked_add(1).ok_or(MigrationError::Conflict)?;
    let scope = Scope {
        plan: new_id,
        revision: new_revision,
        ..old_scope
    };
    let sealed = scope.seal(key, new_id, Purpose::Binding, &binding)?;
    sqlx::query("UPDATE migration_family_refresh_bundle SET state='preparing',revision=$3,digest=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(bundle_revision).execute(&mut *tx).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET state='superseded',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(old_id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO migration_family_refresh_plan(id,bundle_id,organization_id,family,revision,state,phase,source_snapshot_id,history_capture_id,predecessor_plan_id,nonce,ciphertext,original_run_byte_limit,run_byte_limit) VALUES($1,$2,$3,$4,$5,'preparing','mappings',$6,$7,$8,$9,$10,$11,$12)")
        .bind(new_id).bind(id).bind(ctx.organization_id.0).bind(cmd.family.as_str()).bind(new_revision).bind(old.get::<Option<Uuid>,_>("source_snapshot_id")).bind(old.get::<Option<Uuid>,_>("history_capture_id")).bind(old_id).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(old.get::<i64,_>("original_run_byte_limit")).bind(old.get::<i64,_>("run_byte_limit")).execute(&mut *tx).await?;
    let token = Uuid::new_v4();
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=$3,lease_epoch=1,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1 AND organization_id=$2").bind(new_id).bind(ctx.organization_id.0).bind(token).execute(&mut *tx).await?;
    sqlx::query("SELECT set_config('crm.family_refresh_lease',$1,true)")
        .bind(token.to_string())
        .execute(&mut *tx)
        .await?;
    for patch in patches {
        let patch_id = Uuid::new_v4();
        let sealed = scope.seal(key, patch_id, Purpose::MappingPatch, &patch)?;
        let (disposition, target) = patch.choice.columns();
        sqlx::query("INSERT INTO migration_family_refresh_mapping_patch(id,bundle_id,plan_id,organization_id,source_mapping_id,source_plan_id,kind,source_key_hmac,disposition,target_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(patch_id).bind(id).bind(new_id).bind(ctx.organization_id.0).bind(patch.source_mapping).bind(old_id).bind(patch.kind).bind(patch.source_key).bind(disposition).bind(target).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    }
    if let Some(choice) = timezone {
        insert_timezone(&mut tx, key, scope, b.get("source_account_id"), choice).await?;
    }
    let rows=sqlx::query("SELECT id,family,revision FROM migration_family_refresh_plan WHERE bundle_id=$1 AND organization_id=$2 AND state<>'superseded' ORDER BY family").bind(id).bind(ctx.organization_id.0).fetch_all(&mut *tx).await?;
    let mut families = Vec::new();
    for row in rows {
        families.push(PreparedPlan {
            plan_id: row.get("id"),
            family: serde_json::from_value(serde_json::json!(row.get::<String, _>("family")))
                .map_err(|_| MigrationError::Crypto)?,
            revision: row.get::<i64, _>("revision").to_string(),
        });
    }
    families.sort_by_key(|p| p.family);
    let response = PreparedBundle {
        bundle_id: id,
        revision: bundle_revision.to_string(),
        state: "preparing".into(),
        families,
    };
    let sealed = scope.seal(key, cmd.request_id, Purpose::Receipt, &response)?;
    sqlx::query("INSERT INTO migration_family_refresh_receipt(organization_id,actor_user_id,action,request_id,bundle_id,plan_id,input_digest,nonce,ciphertext) VALUES($1,$2,'plan',$3,$4,$5,$6,$7,$8)").bind(ctx.organization_id.0).bind(ctx.actor_user_id.0).bind(cmd.request_id).bind(id).bind(new_id).bind(digest.as_slice()).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *tx).await?;
    let payer: Uuid = b.get("payer_plan_id");
    settle_control(
        &mut tx,
        ctx.organization_id.0,
        id,
        old_id,
        old.get("lease_epoch"),
        old_id != payer,
    )
    .await?;
    if payer != old_id {
        let epoch:i64=sqlx::query_scalar("SELECT lease_epoch FROM migration_family_refresh_plan WHERE id=$1 AND organization_id=$2").bind(payer).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        settle_control(&mut tx, ctx.organization_id.0, id, payer, epoch, false).await?;
    }
    let pending:i64=sqlx::query_scalar("SELECT measured_bytes-retained_bytes FROM migration_family_refresh_plan WHERE id=$1 AND organization_id=$2").bind(new_id).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
    let control = Uuid::new_v4();
    let amount = pending
        .checked_add(8192)
        .ok_or(MigrationError::StorageLimit)?;
    if !sqlx::query_scalar::<_, bool>(
        "SELECT crm_family_refresh_reserve($1,$2,$3,$4,1,$5,'control',$6,$7)",
    )
    .bind(ctx.organization_id.0)
    .bind(id)
    .bind(new_id)
    .bind(control)
    .bind(amount)
    .bind(policy.run_ceiling_bytes)
    .bind(policy.org_ceiling_bytes)
    .fetch_one(&mut *tx)
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    settle_control(&mut tx, ctx.organization_id.0, id, new_id, 1, false).await?;
    sqlx::query("UPDATE migration_family_refresh_plan SET lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2").bind(new_id).bind(ctx.organization_id.0).execute(&mut *tx).await?;
    tx.commit().await?;
    tracing::info!(organization_id=%ctx.organization_id,bundle_id=%id,plan_id=%new_id,revision=new_revision,"Family refresh mapping revision admitted");
    Ok(response)
}

pub(super) fn validate_runtime(binding: &serde_json::Value) -> Result<(), MigrationError> {
    if binding["version"] != ENGINE
        || binding["activity_engine"] != activity_source::ENGINE
        || binding["html_profile"] != activity_source::HTML_PROFILE
        || binding["time_profile"] != activity_source::TIME_PROFILE
        || binding["tzdb"] != activity_source::TZDB_VERSION
    {
        return Err(MigrationError::ReleaseNotReady);
    }
    Ok(())
}
async fn settle_control(
    conn: &mut PgConnection,
    org: Uuid,
    bundle: Uuid,
    plan: Uuid,
    epoch: i64,
    release: bool,
) -> Result<(), MigrationError> {
    let token:Uuid=sqlx::query_scalar("SELECT token FROM migration_family_refresh_reservation WHERE organization_id=$1 AND plan_id=$2 AND purpose='control'").bind(org).bind(plan).fetch_optional(&mut *conn).await?.ok_or(MigrationError::StorageLimit)?;
    sqlx::query("SELECT crm_family_refresh_settle($1,$2,$3,$4,$5,$6)")
        .bind(org)
        .bind(bundle)
        .bind(plan)
        .bind(token)
        .bind(epoch)
        .bind(release)
        .execute(conn)
        .await?;
    Ok(())
}
async fn patches(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    requested: &[MappingPatch],
) -> Result<Vec<Patch>, MigrationError> {
    let mut inputs = Vec::new();
    for patch in requested {
        let row=sqlx::query("SELECT m.*,parent.source_key_hmac AS parent_key FROM migration_family_refresh_mapping m LEFT JOIN migration_family_refresh_mapping parent ON parent.id=m.parent_id AND parent.plan_id=m.plan_id AND parent.organization_id=m.organization_id WHERE m.id=$1 AND m.plan_id=$2 AND m.bundle_id=$3 AND m.organization_id=$4")
            .bind(patch.mapping_id).bind(scope.plan).bind(scope.bundle).bind(scope.organization.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::InvalidImportChoice)?;
        let data: Mapping = scope.open(
            key,
            patch.mapping_id,
            Purpose::Mapping,
            row.get("nonce"),
            row.get("ciphertext"),
        )?;
        data.verify(&row)?;
        if data.kind == "timezone" {
            return Err(MigrationError::InvalidImportChoice);
        }
        inputs.push((patch, row, data));
    }
    inputs.sort_by_key(|(patch, _, data)| (data.kind != "field", patch.mapping_id));
    let mut targets = BTreeMap::<Uuid, Option<Uuid>>::new();
    let mut result = Vec::new();
    for (patch, row, data) in inputs {
        let parent_target = if let Some(parent) = row.get::<Option<Uuid>, _>("parent_id") {
            if let Some(target) = targets.get(&parent) {
                *target
            } else {
                let parent_row=sqlx::query("SELECT *,NULL::bytea AS parent_key FROM migration_family_refresh_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(parent).bind(scope.plan).bind(scope.organization.0).fetch_one(&mut *conn).await?;
                let parent_data: Mapping = scope.open(
                    key,
                    parent,
                    Purpose::Mapping,
                    parent_row.get("nonce"),
                    parent_row.get("ciphertext"),
                )?;
                parent_data.verify(&parent_row)?;
                if parent_data.kind != "field" || Some(parent_data.source_key) != data.parent_key {
                    return Err(MigrationError::Crypto);
                }
                parent_data.choice.columns().1
            }
        } else {
            None
        };
        let source = mapping_selection::source(conn, key, scope, &data).await?;
        let (choice, destination) = mapping_selection::validate(
            conn,
            scope.organization,
            &data,
            &source,
            &patch.choice,
            parent_target,
        )
        .await?;
        targets.insert(patch.mapping_id, choice.columns().1);
        result.push(Patch {
            kind: data.kind,
            source_key: data.source_key,
            source_mapping: patch.mapping_id,
            source_plan: scope.plan,
            choice,
            destination,
        });
    }
    Ok(result)
}
async fn insert_timezone(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    scope: Scope,
    account: i64,
    choice: Choice,
) -> Result<(), MigrationError> {
    let hash = activity_store::source_key(
        key,
        scope.organization,
        account,
        "timezone",
        b"source_timezone",
    );
    let id = Uuid::new_v4();
    let (disposition, target) = choice.columns();
    let data = Mapping {
        kind: "timezone".into(),
        source_key: hash.clone(),
        parent_key: None,
        source: None,
        label: match &choice {
            Choice::Timezone { zone } => Some(zone.clone()),
            _ => None,
        },
        qualified: true,
        creation_allowed: false,
        choice,
        destination: None,
    };
    let sealed = scope.seal(key, id, Purpose::Mapping, &data)?;
    sqlx::query("INSERT INTO migration_family_refresh_mapping(id,bundle_id,plan_id,organization_id,kind,source_key_hmac,disposition,target_id,nonce,ciphertext,qualified) VALUES($1,$2,$3,$4,'timezone',$5,$6,$7,$8,$9,true)")
        .bind(id).bind(scope.bundle).bind(scope.plan).bind(scope.organization.0).bind(hash).bind(disposition).bind(target).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(conn).await?;
    Ok(())
}

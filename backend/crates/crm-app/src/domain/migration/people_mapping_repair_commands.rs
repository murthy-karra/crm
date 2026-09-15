//! Repair draft creation and immutable choice editing, under existing refresh ownership.
use super::{
    admitted_people_refresh, admitted_people_refresh_store as a,
    people_mapping_repair::{self as repair, Owner, SourceKey},
    people_refresh_store as o,
    snapshot::SnapshotPolicy,
    MigrationError,
};
use crate::{
    auth::workspace::ReleaseReadiness, config::RawPayloadKey, domain::envelope::CommandContext,
    ids::OrganizationId,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Anchor {
    Results { plan_id: Uuid },
    Preview { plan_id: Uuid, plan_revision: i64 },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub request_id: Uuid,
    pub report_id: Uuid,
    pub expected_lifecycle_revision: i64,
    pub anchor: Anchor,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub key_id: Uuid,
    pub disposition: String,
    pub target_id: Option<Uuid>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditChoices {
    pub request_id: Uuid,
    pub expected_draft_revision: i64,
    pub choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeRepair {
    pub choices_digest: String,
    pub candidate_count: i64,
    pub approval_only_count: i64,
    pub unassigned_count: i64,
}

/// A lifecycle revision covers every choice edit. Immutable catalog and choice
/// rows make this revision a stable commitment without loading all choices.
pub(crate) async fn freeze_choices(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    org: OrganizationId,
    root: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    if !root.get::<bool, _>("repair_candidates_complete")
        || root.get::<i64, _>("repair_candidate_count") == 0
    {
        return Err(MigrationError::Conflict);
    }
    let d=super::crypto::snapshot_hmac(key,org,"repair-choices-v1",&serde_json::to_vec(&json!({"root":root.get::<Uuid,_>("id"),"revision":root.get::<i64,_>("repair_draft_revision"),"count":root.get::<i64,_>("repair_candidate_count")})).map_err(|_|MigrationError::Crypto)?);
    let p = owner.prefix();
    sqlx::query(&format!("UPDATE {p} SET repair_frozen_revision=repair_draft_revision,repair_choices_digest=$3,pause_reason=NULL WHERE id=$1 AND organization_id=$2"))
        .bind(root.get::<Uuid,_>("id")).bind(org.0).bind(d.as_slice()).execute(conn).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Match the existing typed command and receipt boundary.
pub(crate) async fn repreview_repair(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    owner: Owner,
    ctx: &CommandContext,
    root: &sqlx::postgres::PgRow,
    request: Uuid,
    expected_revision: i64,
    request_digest: &[u8; 32],
) -> Result<Value, MigrationError> {
    let org = ctx.organization_id;
    let id = root.get::<Uuid, _>("id");
    let p = owner.prefix();
    if root.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0
        || root.get::<i64, _>("lifecycle_revision") != expected_revision
        || root
            .get::<Option<Uuid>, _>("confirmed_refresh_plan_id")
            .is_some()
        || !(root.get::<String, _>("state") == "ready"
            || (root.get::<String, _>("state") == "paused"
                && root.get::<Option<String>, _>("pause_reason").as_deref()
                    == Some("awaiting_mapping_choices")))
    {
        return Err(MigrationError::Conflict);
    }
    source_boundary(conn, owner, org, root).await?;
    let token = reserve(
        conn,
        owner,
        org,
        id,
        "prepare",
        8192,
        &SnapshotPolicy::default(),
    )
    .await?;
    let before = measured(conn, owner, org, id).await?;
    freeze_choices(conn, key, owner, org, root).await?;
    sqlx::query(&format!("UPDATE {p}_plan SET state='superseded' WHERE refresh_id=$1 AND organization_id=$2 AND state='ready'"))
        .bind(id).bind(org.0).execute(&mut *conn).await?;
    let phase = if owner == Owner::Original {
        "imported"
    } else {
        "admission_results"
    };
    sqlx::query(&format!("UPDATE {p} SET state='preparing',preparation_phase=$3,preparation_checkpoint_key='',lifecycle_revision=lifecycle_revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2"))
        .bind(id).bind(org.0).bind(phase).execute(&mut *conn).await?;
    let value = json!({"refresh_id":id,"state":"preparing"});
    receipt(
        conn,
        owner,
        key,
        ctx,
        "repreview",
        request,
        id,
        request_digest,
        &value,
    )
    .await?;
    let after = measured(conn, owner, org, id).await?;
    release(conn, owner, org, id, token, after - before).await?;
    Ok(value)
}

pub(crate) async fn confirm_guard(
    conn: &mut PgConnection,
    owner: Owner,
    org: OrganizationId,
    root: &sqlx::postgres::PgRow,
    plan: &sqlx::postgres::PgRow,
    ack: Option<&AcknowledgeRepair>,
    release: Option<&ReleaseReadiness>,
) -> Result<(), MigrationError> {
    source_boundary(conn, owner, org, root).await?;
    if root
        .get::<Option<Uuid>, _>("repair_source_refresh_id")
        .is_none()
    {
        return if ack.is_none() {
            Ok(())
        } else {
            Err(MigrationError::InvalidInput)
        };
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_mapping_repair(conn)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let ack = ack.ok_or(MigrationError::Conflict)?;
    let digest = plan
        .get::<Option<Vec<u8>>, _>("repair_choices_digest")
        .ok_or(MigrationError::Conflict)?;
    let hex = digest
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    if ack.choices_digest != hex
        || ack.candidate_count != plan.get::<i64, _>("repair_candidate_count")
        || ack.approval_only_count != plan.get::<i64, _>("repair_approval_only_count")
        || ack.unassigned_count != plan.get::<i64, _>("repair_unassigned_count")
        || root.get::<Option<Vec<u8>>, _>("repair_choices_digest") != Some(digest)
        || root.get::<Option<i64>, _>("repair_frozen_revision")
            != plan.get::<Option<i64>, _>("repair_choices_revision")
    {
        return Err(MigrationError::Conflict);
    }
    sqlx::query("INSERT INTO migration_mapping_repair_requirement(organization_id,capability) VALUES($1,$2) ON CONFLICT(organization_id) DO NOTHING").bind(org.0).bind(repair::CAPABILITY).execute(conn).await?;
    Ok(())
}

pub(crate) async fn reserve(
    conn: &mut PgConnection,
    owner: Owner,
    org: OrganizationId,
    root: Uuid,
    purpose: &str,
    amount: i64,
    policy: &SnapshotPolicy,
) -> Result<Uuid, MigrationError> {
    match owner {
        Owner::Original => o::reserve(conn, org, root, purpose, None, amount, policy).await,
        Owner::Admitted => a::reserve(conn, org, root, purpose, None, amount, policy).await,
    }
}
pub(crate) async fn release(
    conn: &mut PgConnection,
    owner: Owner,
    org: OrganizationId,
    root: Uuid,
    token: Uuid,
    actual: i64,
) -> Result<(), MigrationError> {
    match owner {
        Owner::Original => o::release(conn, org, root, token, actual).await,
        Owner::Admitted => a::release(conn, org, root, token, actual).await,
    }
}
pub(crate) async fn measured(
    conn: &mut PgConnection,
    owner: Owner,
    org: OrganizationId,
    root: Uuid,
) -> Result<i64, MigrationError> {
    match owner {
        Owner::Original => o::measured_bytes(conn, org, root).await,
        Owner::Admitted => a::measured_bytes(conn, org, root).await,
    }
}
fn digest<T: Serialize>(
    owner: Owner,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    root: Uuid,
    body: &T,
) -> Result<[u8; 32], MigrationError> {
    match owner {
        Owner::Original => o::digest(key, ctx, action, Some(root), body),
        Owner::Admitted => a::digest(key, ctx, action, Some(root), body),
    }
}
async fn replay(
    conn: &mut PgConnection,
    owner: Owner,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    digest: &[u8; 32],
) -> Result<Option<Value>, MigrationError> {
    match owner {
        Owner::Original => o::replay(conn, key, ctx, action, request, digest).await,
        Owner::Admitted => a::replay(conn, key, ctx, action, request, digest).await,
    }
}
#[allow(clippy::too_many_arguments)] // Match the existing typed command and receipt boundary.
async fn receipt(
    conn: &mut PgConnection,
    owner: Owner,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    action: &str,
    request: Uuid,
    root: Uuid,
    digest: &[u8; 32],
    value: &Value,
) -> Result<(), MigrationError> {
    match owner {
        Owner::Original => o::receipt(conn, key, ctx, action, request, root, digest, value).await,
        Owner::Admitted => a::receipt(conn, key, ctx, action, request, root, digest, value).await,
    }
}

/// Caller already holds the Organization row before inspecting owner roots.
pub(crate) async fn source_boundary(
    conn: &mut PgConnection,
    owner: Owner,
    org: OrganizationId,
    root: &sqlx::postgres::PgRow,
) -> Result<(), MigrationError> {
    let p = owner.prefix();
    let cohort = owner.cohort_column();
    let prior=sqlx::query(&format!("SELECT confirmed_snapshot_id,confirmed_completed_at,report_id,newer_sequence FROM {p} WHERE organization_id=$1 AND {cohort}=$2 AND confirmed_snapshot_id IS NOT NULL AND confirmed_completed_at IS NOT NULL ORDER BY confirmed_completed_at DESC,id DESC LIMIT 1 FOR SHARE"))
        .bind(org.0).bind(root.get::<Uuid,_>(cohort)).fetch_optional(conn).await?;
    if let Some(prior) = prior {
        let exact = prior.get::<Uuid, _>("confirmed_snapshot_id")
            == root.get::<Uuid, _>("newer_snapshot_id")
            && prior.get::<i64, _>("newer_sequence") == root.get::<i64, _>("newer_sequence")
            && (owner == Owner::Original
                || prior.get::<Uuid, _>("report_id") == root.get::<Uuid, _>("report_id"));
        if !exact
            && root.get::<chrono::DateTime<chrono::Utc>, _>("newer_started_at")
                <= prior.get::<chrono::DateTime<chrono::Utc>, _>("confirmed_completed_at")
        {
            return Err(MigrationError::Conflict);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Match the existing typed command and receipt boundary.
pub async fn prepare(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    owner: Owner,
    source_id: Uuid,
    cmd: Prepare,
    evidence: Option<&ReleaseReadiness>,
) -> Result<Value, MigrationError> {
    let mut tx = o::begin(pool, ctx).await?;
    let d = digest(owner, key, ctx, "mapping_repair", source_id, &cmd)?;
    if let Some(value) = replay(
        &mut tx,
        owner,
        key,
        ctx,
        "mapping_repair",
        cmd.request_id,
        &d,
    )
    .await?
    {
        return Ok(value);
    }
    let evidence = evidence.ok_or(MigrationError::ReleaseNotReady)?;
    evidence
        .require_mapping_repair(&mut tx)
        .await
        .map_err(|_| MigrationError::ReleaseNotReady)?;
    let p = owner.prefix();
    let org = ctx.organization_id;
    let source = sqlx::query(&format!(
        "SELECT * FROM {p} WHERE id=$1 AND organization_id=$2 FOR UPDATE"
    ))
    .bind(source_id)
    .bind(org.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if source.get::<i64, _>("lifecycle_revision") != cmd.expected_lifecycle_revision {
        return Err(MigrationError::Conflict);
    }
    let state = source.get::<String, _>("state");
    let confirmed = source.get::<Option<Uuid>, _>("confirmed_refresh_plan_id");
    let (plan_id, anchor_kind) = match cmd.anchor {
        Anchor::Results { plan_id }
            if matches!(state.as_str(), "completed" | "cancelled")
                && confirmed == Some(plan_id) =>
        {
            (plan_id, "results")
        }
        Anchor::Preview {
            plan_id,
            plan_revision,
        } if matches!(state.as_str(), "ready" | "cancelled") && confirmed.is_none() => {
            let valid:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {p}_plan WHERE id=$1 AND refresh_id=$2 AND organization_id=$3 AND revision=$4 AND sealed_at IS NOT NULL AND state IN ('ready','expired'))"))
                .bind(plan_id).bind(source_id).bind(org.0).bind(plan_revision).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(MigrationError::Conflict);
            }
            (plan_id, "preview")
        }
        _ => return Err(MigrationError::Conflict),
    };
    let remainder = state == "cancelled"
        && confirmed == Some(plan_id)
        && source
            .get::<Option<Uuid>, _>("repair_source_refresh_id")
            .is_some();
    if remainder && cmd.report_id != source.get::<Uuid, _>("report_id") {
        return Err(MigrationError::SourceNotEligible);
    }
    let has_candidates:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {p}_item i WHERE i.refresh_id=$1 AND i.plan_id=$2 AND i.organization_id=$3 AND i.source_id IS NOT NULL AND (($5 AND i.settled_at IS NULL AND i.disposition IN ('eligible','already_current')) OR (NOT $5 AND (i.disposition='held_mapping_gap' OR ($4='results' AND EXISTS(SELECT 1 FROM {p}_result r WHERE r.item_id=i.id AND r.refresh_id=i.refresh_id AND r.organization_id=i.organization_id AND r.disposition IN ('held_mapping_gap','held_stale')))))))"))
        .bind(source_id).bind(plan_id).bind(org.0).bind(anchor_kind).bind(remainder).fetch_one(&mut *tx).await?;
    if !has_candidates {
        return Err(MigrationError::SourceNotEligible);
    }
    if state == "ready" {
        sqlx::query(&format!("UPDATE {p} SET state='cancelled',cancelled_at=clock_timestamp(),lifecycle_revision=lifecycle_revision+1,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2"))
            .bind(source_id).bind(org.0).execute(&mut *tx).await?;
        let reservations:Vec<Uuid>=sqlx::query_scalar(&format!("SELECT token FROM {p}_reservation WHERE refresh_id=$1 AND organization_id=$2 FOR UPDATE"))
            .bind(source_id).bind(org.0).fetch_all(&mut *tx).await?;
        for token in reservations {
            release(&mut tx, owner, org, source_id, token, 0).await?;
        }
    }
    if owner == Owner::Admitted {
        admitted_people_refresh::qualify_repair_report(
            &mut tx,
            key,
            org.0,
            source.get("admission_id"),
            cmd.report_id,
            Some(evidence),
        )
        .await?;
    } else {
        evidence
            .require_people_refresh(&mut tx)
            .await
            .map_err(|_| MigrationError::ReleaseNotReady)?;
    }
    let report=sqlx::query("SELECT r.*,s.started_at AS newer_started_at,s.completed_at AS newer_completed_at,i.state AS parent_state,i.confirmed_plan_id,o.workspace_mode,o.workspace_revision AS current_workspace_revision FROM migration_core_change_report r JOIN migration_snapshot s ON s.id=r.newer_snapshot_id AND s.organization_id=r.organization_id JOIN migration_import i ON i.id=r.parent_import_id AND i.organization_id=r.organization_id JOIN organization o ON o.id=r.organization_id WHERE r.id=$1 AND r.organization_id=$2 FOR SHARE")
        .bind(cmd.report_id).bind(org.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if report.get::<String, _>("state") != "completed"
        || report.get::<String, _>("parent_state") != "completed"
        || report.get::<String, _>("workspace_mode") != "migration_review"
        || report.get::<Uuid, _>("parent_import_id") != source.get::<Uuid, _>("parent_import_id")
        || report.get::<Uuid, _>("parent_plan_id") != source.get::<Uuid, _>("parent_plan_id")
        || report.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(source.get("parent_plan_id"))
        || report.get::<i64, _>("source_account_id") != source.get::<i64, _>("source_account_id")
        || report.get::<i64, _>("current_workspace_revision")
            != source.get::<i64, _>("workspace_revision")
    {
        return Err(MigrationError::SourceNotEligible);
    }
    super::core_change_store::validate(&mut tx, key, org, &report).await?;
    let started = report
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("newer_started_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    let completed = report
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("newer_completed_at")
        .ok_or(MigrationError::SourceNotEligible)?;
    let id = Uuid::new_v4();
    let (extra_column, extra_value) = if owner == Owner::Admitted {
        (",admission_id", ",$15")
    } else {
        ("", "")
    };
    let sql=format!("INSERT INTO {p}(id,organization_id,parent_import_id,parent_plan_id,report_id,source_account_id,newer_snapshot_id,newer_sequence,newer_started_at,newer_completed_at,workspace_revision,initiated_by_user_id,engine_version,state,repair_source_refresh_id,repair_source_plan_id,repair_anchor_kind{extra_column}) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'{}','preparing',$13,$14,'{anchor_kind}'{extra_value})",if owner==Owner::Original{o::ENGINE}else{a::ENGINE});
    let mut insert = sqlx::query(&sql)
        .bind(id)
        .bind(org.0)
        .bind(source.get::<Uuid, _>("parent_import_id"))
        .bind(source.get::<Uuid, _>("parent_plan_id"))
        .bind(cmd.report_id)
        .bind(source.get::<i64, _>("source_account_id"))
        .bind(report.get::<Uuid, _>("newer_snapshot_id"))
        .bind(report.get::<i64, _>("newer_sequence"))
        .bind(started)
        .bind(completed)
        .bind(source.get::<i64, _>("workspace_revision"))
        .bind(ctx.actor_user_id.0)
        .bind(source_id)
        .bind(plan_id);
    if owner == Owner::Admitted {
        insert = insert.bind(source.get::<Uuid, _>("admission_id"));
    }
    insert.execute(&mut *tx).await?;
    let new_root = sqlx::query(&format!(
        "SELECT * FROM {p} WHERE id=$1 AND organization_id=$2"
    ))
    .bind(id)
    .bind(org.0)
    .fetch_one(&mut *tx)
    .await?;
    source_boundary(&mut tx, owner, org, &new_root).await?;
    reserve(&mut tx, owner, org, id, "cancel", 64 * 1024, policy).await?;
    let token = reserve(&mut tx, owner, org, id, "prepare", 8 * 1024, policy).await?;
    let before = measured(&mut tx, owner, org, id).await?;
    let value = json!({"refresh_id":id,"state":"preparing","mode":"mapping_repair","source_refresh_id":source_id});
    receipt(
        &mut tx,
        owner,
        key,
        ctx,
        "mapping_repair",
        cmd.request_id,
        id,
        &d,
        &value,
    )
    .await?;
    let after = measured(&mut tx, owner, org, id).await?;
    release(&mut tx, owner, org, id, token, after - before).await?;
    tx.commit().await?;
    Ok(value)
}

pub async fn edit_choices(
    pool: &PgPool,
    key: &RawPayloadKey,
    policy: &SnapshotPolicy,
    ctx: &CommandContext,
    owner: Owner,
    id: Uuid,
    cmd: EditChoices,
) -> Result<Value, MigrationError> {
    if cmd.choices.is_empty()
        || cmd.choices.len() > 50
        || serde_json::to_vec(&cmd)
            .map_err(|_| MigrationError::InvalidInput)?
            .len()
            > 128 * 1024
    {
        return Err(MigrationError::InvalidInput);
    }
    let mut tx = o::begin(pool, ctx).await?;
    let org = ctx.organization_id;
    let p = owner.prefix();
    let d = digest(owner, key, ctx, "mapping_choices", id, &cmd)?;
    if let Some(value) = replay(
        &mut tx,
        owner,
        key,
        ctx,
        "mapping_choices",
        cmd.request_id,
        &d,
    )
    .await?
    {
        return Ok(value);
    }
    let root = sqlx::query(&format!(
        "SELECT * FROM {p} WHERE id=$1 AND organization_id=$2 FOR UPDATE"
    ))
    .bind(id)
    .bind(org.0)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(MigrationError::NotFound)?;
    if root.get::<Uuid, _>("initiated_by_user_id") != ctx.actor_user_id.0 {
        return Err(MigrationError::Forbidden);
    }
    if root
        .get::<Option<Uuid>, _>("repair_source_refresh_id")
        .is_none()
        || root
            .get::<Option<Uuid>, _>("confirmed_refresh_plan_id")
            .is_some()
        || root.get::<i64, _>("repair_draft_revision") != cmd.expected_draft_revision
        || !root.get::<bool, _>("repair_candidates_complete")
        || !(root.get::<String, _>("state") == "ready"
            || (root.get::<String, _>("state") == "paused"
                && root.get::<Option<String>, _>("pause_reason").as_deref()
                    == Some("awaiting_mapping_choices")))
    {
        return Err(MigrationError::Conflict);
    }
    let token = reserve(&mut tx, owner, org, id, "prepare", 8 * 1024 * 1024, policy).await?;
    let before = measured(&mut tx, owner, org, id).await?;
    let revision = cmd
        .expected_draft_revision
        .checked_add(1)
        .ok_or(MigrationError::Conflict)?;
    let mut seen = std::collections::HashSet::new();
    for choice in &cmd.choices {
        if !seen.insert(choice.key_id) {
            return Err(MigrationError::InvalidInput);
        }
        let entry = sqlx::query(&format!(
            "SELECT * FROM {p}_repair_key WHERE id=$1 AND refresh_id=$2 AND organization_id=$3"
        ))
        .bind(choice.key_id)
        .bind(id)
        .bind(org.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MigrationError::NotFound)?;
        let source: SourceKey = owner.open(
            key,
            org,
            id,
            entry.get("id"),
            "repair-key",
            &entry.get::<Vec<u8>, _>("nonce"),
            &entry.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let target = if choice.disposition == "unresolved" && choice.target_id.is_none() {
            Value::Null
        } else {
            repair::target_snapshot(
                &mut tx,
                org,
                source.kind(),
                &choice.disposition,
                choice.target_id,
            )
            .await?
        };
        let choice_id = Uuid::new_v4();
        let body = owner.seal(
            key,
            org,
            id,
            choice_id,
            "repair-choice",
            &json!({"source":source,"target":target}),
        )?;
        if body.ciphertext.len() > 65536 {
            return Err(MigrationError::StorageLimit);
        }
        sqlx::query(&format!("INSERT INTO {p}_repair_choice(id,refresh_id,organization_id,revision,kind,source_key_hmac,disposition,target_id,nonce,ciphertext,approved_by_user_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"))
            .bind(choice_id).bind(id).bind(org.0).bind(revision).bind(source.kind()).bind(source.digest(key,org)?.as_slice()).bind(&choice.disposition).bind(choice.target_id)
            .bind(body.nonce.as_slice()).bind(body.ciphertext).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
    }
    sqlx::query(&format!("UPDATE {p}_plan SET state='superseded' WHERE refresh_id=$1 AND organization_id=$2 AND state='ready'"))
        .bind(id).bind(org.0).execute(&mut *tx).await?;
    sqlx::query(&format!("UPDATE {p} SET repair_draft_revision=$3,repair_frozen_revision=NULL,repair_choices_digest=NULL,state='paused',pause_reason='awaiting_mapping_choices',lifecycle_revision=lifecycle_revision+1,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2"))
        .bind(id).bind(org.0).bind(revision).execute(&mut *tx).await?;
    let value = json!({"refresh_id":id,"state":"paused","pause_reason":"awaiting_mapping_choices","draft_revision":revision.to_string()});
    receipt(
        &mut tx,
        owner,
        key,
        ctx,
        "mapping_choices",
        cmd.request_id,
        id,
        &d,
        &value,
    )
    .await?;
    let after = measured(&mut tx, owner, org, id).await?;
    release(&mut tx, owner, org, id, token, after - before).await?;
    tx.commit().await?;
    Ok(value)
}

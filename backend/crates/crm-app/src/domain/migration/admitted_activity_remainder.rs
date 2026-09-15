//! Exact successor construction for a confirmed activity attempt cancelled before
//! every manifest settled.  It only re-encrypts retained, immutable plan data.
use super::{
    admitted_activity::{self, Patch},
    admitted_activity_model::{CapturedRecord, Counts, Manifest, Mapping},
    admitted_activity_store as s, MigrationError,
};
use crate::{config::RawPayloadKey, domain::envelope::CommandContext};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAdmittedActivityRemainder {
    pub request_id: Uuid,
    pub expected_revision: String,
}

pub async fn create(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    predecessor: Uuid,
    cmd: CreateAdmittedActivityRemainder,
    policy: &super::snapshot::SnapshotPolicy,
) -> Result<Value, MigrationError> {
    s::with_policy(policy, async {
        let mut tx = s::begin(pool, ctx, false).await?;
        if let Some(receipt) = s::replay(
            &mut tx, key, ctx, "remainder", cmd.request_id, &(predecessor, &cmd),
        ).await? {
            return Ok(receipt);
        }
        let old = s::run(&mut tx, ctx.organization_id, predecessor).await?;
        if old.get::<String, _>("state") != "cancelled"
            || old.get::<Option<Uuid>, _>("confirmed_plan_id").is_none()
            || old.get::<i64, _>("revision").to_string() != cmd.expected_revision
        {
            return Err(MigrationError::ImportConflict);
        }
        admitted_activity::validate_binding(&mut tx, &old).await?;
        if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM migration_admitted_activity_import WHERE predecessor_import_id=$1 AND organization_id=$2)")
            .bind(predecessor).bind(ctx.organization_id.0).fetch_one(&mut *tx).await? {
            return Err(MigrationError::ImportConflict);
        }
        let (source_import, old_plan) = copy_source(&old)?;
        let source = s::run(&mut tx, ctx.organization_id, source_import).await?;
        let base = s::plan(&mut tx, ctx.organization_id, source_import, old_plan).await?;
        if !has_pending(&source)? { return Err(MigrationError::ImportConflict); }
        let id = Uuid::new_v4();
        let plan = Uuid::new_v4();
        let snapshot: Uuid = old.get("snapshot_id");
        let cancel = Uuid::new_v4();
        let dst = admitted_activity::decode_destination(key, &source, &base)?;
        let patch = s::seal(key, ctx.organization_id, snapshot, plan, plan, "patch", &Vec::<Patch>::new())?;
        let destination = s::seal(key, ctx.organization_id, snapshot, plan, plan, "destination", &dst)?;
        sqlx::query("INSERT INTO migration_admitted_activity_import(id,organization_id,parent_import_id,parent_plan_id,snapshot_id,preview_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,state,phase,latest_plan_id,confirmed_plan_id,cancel_reservation_token,counts,admission_id,admission_plan_id,source_report_id,source_output_revision,predecessor_import_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'preparing','preparation',NULL,NULL,$11,$12,$13,$14,$15,$16,$17)")
            .bind(id).bind(ctx.organization_id.0).bind(old.get::<Uuid,_>("parent_import_id")).bind(old.get::<Uuid,_>("parent_plan_id")).bind(snapshot).bind(old.get::<Uuid,_>("preview_id")).bind(old.get::<i64,_>("source_account_id")).bind(old.get::<i64,_>("capture_sequence")).bind(old.get::<i64,_>("workspace_revision")).bind(ctx.actor_user_id.0).bind(cancel).bind(json!(Counts::default())).bind(old.get::<Uuid,_>("admission_id")).bind(old.get::<Uuid,_>("admission_plan_id")).bind(old.get::<Uuid,_>("source_report_id")).bind(old.get::<Uuid,_>("source_output_revision")).bind(predecessor).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO migration_admitted_activity_plan(id,import_id,snapshot_id,organization_id,revision,state,phase,patch_nonce,patch_ciphertext,destination_nonce,destination_ciphertext,counts,expires_at) VALUES($1,$2,$3,$4,1,'building','copying_choices',$5,$6,$7,$8,$9,now()+interval '10 minutes')")
            .bind(plan).bind(id).bind(snapshot).bind(ctx.organization_id.0).bind(patch.nonce.as_slice()).bind(patch.ciphertext).bind(destination.nonce.as_slice()).bind(destination.ciphertext).bind(json!(Counts::default())).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_activity_import SET latest_plan_id=$3,confirmed_plan_id=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(plan).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_activity_import SET remainder_source_import_id=$3,remainder_source_plan_id=$4 WHERE id=$1 AND organization_id=$2")
            .bind(id).bind(ctx.organization_id.0).bind(source_import).bind(old_plan).execute(&mut *tx).await?;
        if !s::reserve(&mut tx,ctx.organization_id,id,snapshot,plan,cancel,cancel,s::CANCEL_RESERVATION,"cancel").await? { return Err(MigrationError::StorageLimit); }
        let r=s::run(&mut tx,ctx.organization_id,id).await?; let mut response=json!({"import":admitted_activity::view(&mut tx,key,ctx.organization_id,&r).await?});
        s::receipt(&mut tx,key,ctx,"remainder",cmd.request_id,&(predecessor,&cmd),id,snapshot,plan,&mut response).await?;
        tx.commit().await?; Ok(response)
    }).await
}

/// Counts are committed atomically with every native result/checkpoint. The
/// canonical cancelled source is stable, including while a successor copy is
/// incomplete. Availability never scans that source's manifest or result tail.
pub(crate) fn has_pending(row: &sqlx::postgres::PgRow) -> Result<bool, MigrationError> {
    let counts = Counts::load(row.get("counts"))?;
    Ok(counts.notes.pending > 0 || counts.tasks.pending > 0)
}

/// A cancelled partial copy continues from its complete immutable source plan,
/// not from the partially copied rows. No recursive chain scan is required.
pub(crate) fn copy_source(row: &sqlx::postgres::PgRow) -> Result<(Uuid, Uuid), MigrationError> {
    if row.get::<String, _>("phase") == "preparation"
        && row
            .get::<Option<Uuid>, _>("predecessor_import_id")
            .is_some()
    {
        Ok((
            row.get::<Option<Uuid>, _>("remainder_source_import_id")
                .ok_or(MigrationError::SourceNotEligible)?,
            row.get::<Option<Uuid>, _>("remainder_source_plan_id")
                .ok_or(MigrationError::SourceNotEligible)?,
        ))
    } else {
        Ok((
            row.get("id"),
            row.get::<Option<Uuid>, _>("confirmed_plan_id")
                .ok_or(MigrationError::SourceNotEligible)?,
        ))
    }
}

/// One retained row per fenced worker transaction. No native row is eligible
/// until all three copies and their byte settlements are durably complete.
pub(crate) async fn copy_unit(
    conn: &mut PgConnection,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    r: &sqlx::postgres::PgRow,
    p: &sqlx::postgres::PgRow,
    lease: Uuid,
) -> Result<(), MigrationError> {
    let id: Uuid = r.get("id");
    let plan: Uuid = p.get("id");
    let snapshot: Uuid = r.get("snapshot_id");
    let (source_import, old_plan) = copy_source(r)?;
    let source = s::run(conn, ctx.organization_id, source_import).await?;
    if source.get::<String, _>("state") != "cancelled"
        || source.get::<Option<Uuid>, _>("confirmed_plan_id") != Some(old_plan)
    {
        return Err(MigrationError::SourceNotEligible);
    }
    admitted_activity::validate_binding(conn, &source).await?;
    let token = Uuid::new_v4();
    if !s::reserve(
        conn,
        ctx.organization_id,
        id,
        snapshot,
        plan,
        lease,
        token,
        s::UNIT,
        "work",
    )
    .await?
    {
        return Err(MigrationError::StorageLimit);
    }
    let after = p
        .get::<Option<Uuid>, _>("checkpoint_id")
        .unwrap_or(Uuid::nil());
    let phase = p.get::<String, _>("phase");
    let row=match phase.as_str() {
        "copying_choices" => sqlx::query("SELECT * FROM migration_admitted_activity_mapping WHERE plan_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 1").bind(old_plan).bind(ctx.organization_id.0).bind(after).fetch_optional(&mut *conn).await?,
        "captures" => sqlx::query("SELECT * FROM migration_admitted_activity_source WHERE plan_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 1").bind(old_plan).bind(ctx.organization_id.0).bind(after).fetch_optional(&mut *conn).await?,
        "manifests" => sqlx::query("SELECT * FROM migration_admitted_activity_manifest WHERE plan_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 1").bind(old_plan).bind(ctx.organization_id.0).bind(after).fetch_optional(&mut *conn).await?,
        _ => return Err(MigrationError::SourceNotEligible),
    };
    if let Some(row) = row {
        let old_id: Uuid = row.get("id");
        match phase.as_str() {
            "copying_choices" => {
                let new_id = Uuid::new_v4();
                let old_id: Uuid = row.get("id");
                let data: Mapping = s::open(
                    key,
                    ctx.organization_id,
                    snapshot,
                    old_plan,
                    old_id,
                    "mapping",
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                let sealed = s::seal(
                    key,
                    ctx.organization_id,
                    snapshot,
                    plan,
                    new_id,
                    "mapping",
                    &data,
                )?;
                sqlx::query("INSERT INTO migration_admitted_activity_mapping(id,plan_id,import_id,organization_id,kind,source_key,dependent_count,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(new_id).bind(plan).bind(id).bind(ctx.organization_id.0).bind(row.get::<String,_>("kind")).bind(row.get::<Vec<u8>,_>("source_key")).bind(row.get::<i64,_>("dependent_count")).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
            }
            "captures" => {
                let old_id: Uuid = row.get("id");
                let new_id = Uuid::new_v4();
                let data: CapturedRecord = s::open(
                    key,
                    ctx.organization_id,
                    snapshot,
                    old_plan,
                    old_id,
                    "source",
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                let sealed = s::seal(
                    key,
                    ctx.organization_id,
                    snapshot,
                    plan,
                    new_id,
                    "source",
                    &data,
                )?;
                sqlx::query("INSERT INTO migration_admitted_activity_source(id,plan_id,import_id,snapshot_id,organization_id,family,source_id,record_id,capture_id,capture_sequence,ordinal,stream,representation,semantic_hmac,negative,source_only_counts,nonce,ciphertext,source_person_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)").bind(new_id).bind(plan).bind(id).bind(snapshot).bind(ctx.organization_id.0).bind(row.get::<String,_>("family")).bind(row.get::<Option<String>,_>("source_id")).bind(row.get::<Option<Uuid>,_>("record_id")).bind(row.get::<Uuid,_>("capture_id")).bind(row.get::<i64,_>("capture_sequence")).bind(row.get::<i32,_>("ordinal")).bind(row.get::<String,_>("stream")).bind(row.get::<String,_>("representation")).bind(row.get::<Vec<u8>,_>("semantic_hmac")).bind(row.get::<bool,_>("negative")).bind(row.get::<Value,_>("source_only_counts")).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).bind(row.get::<Option<String>,_>("source_person_id")).execute(&mut *conn).await?;
            }
            // Even a fully settled tail advances one checkpoint per unit. An
            // anti-join inside LIMIT 1 could walk the whole tail in one unit.
            "manifests" if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM migration_admitted_activity_result WHERE import_id=$1 AND organization_id=$2 AND manifest_id=$3)")
                .bind(source_import).bind(ctx.organization_id.0).bind(old_id).fetch_one(&mut *conn).await? => {}
            "manifests" => {
                let mut counts = Counts::load(p.get("counts"))?;
                let source_row:Uuid=sqlx::query_scalar("SELECT c.id FROM migration_admitted_activity_source a JOIN migration_admitted_activity_source c ON c.plan_id=$1 AND c.organization_id=a.organization_id AND c.capture_id=a.capture_id AND c.ordinal=a.ordinal WHERE a.id=$2 AND a.plan_id=$3 AND a.organization_id=$4").bind(plan).bind(row.get::<Uuid,_>("source_row_id")).bind(old_plan).bind(ctx.organization_id.0).fetch_optional(&mut *conn).await?.ok_or(MigrationError::SourceNotEligible)?;
                let old_id: Uuid = row.get("id");
                let new_id = Uuid::new_v4();
                let data: Manifest = s::open(
                    key,
                    ctx.organization_id,
                    snapshot,
                    old_plan,
                    old_id,
                    "manifest",
                    row.get("nonce"),
                    row.get("ciphertext"),
                )?;
                let sealed = s::seal(
                    key,
                    ctx.organization_id,
                    snapshot,
                    plan,
                    new_id,
                    "manifest",
                    &data,
                )?;
                let disposition: String = row.get("disposition");
                let added: i64 = row.get("added_byte_bound");
                if disposition != "excluded" {
                    counts.planned(row.get("kind"), &disposition, data.source_only_count);
                } else if row.get::<Option<String>, _>("source_id").is_some() {
                    counts.excluded_count += 1;
                } else {
                    counts.invalid_occurrences += 1;
                }
                sqlx::query("INSERT INTO migration_admitted_activity_manifest(id,plan_id,import_id,organization_id,kind,source_id,source_row_id,source_person_id,admission_result_id,person_id,target_id,native_source_key,author_user_id,creator_user_id,assignee_user_id,disposition,added_byte_bound,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)").bind(new_id).bind(plan).bind(id).bind(ctx.organization_id.0).bind(row.get::<String,_>("kind")).bind(row.get::<Option<String>,_>("source_id")).bind(source_row).bind(row.get::<Option<String>,_>("source_person_id")).bind(row.get::<Option<Uuid>,_>("admission_result_id")).bind(row.get::<Option<Uuid>,_>("person_id")).bind(row.get::<Option<Uuid>,_>("target_id")).bind(row.get::<Option<String>,_>("native_source_key")).bind(row.get::<Option<Uuid>,_>("author_user_id")).bind(row.get::<Option<Uuid>,_>("creator_user_id")).bind(row.get::<Option<Uuid>,_>("assignee_user_id")).bind(disposition).bind(added).bind(sealed.nonce.as_slice()).bind(sealed.ciphertext).execute(&mut *conn).await?;
                sqlx::query("INSERT INTO migration_admitted_activity_manifest_issue(manifest_id,plan_id,import_id,organization_id,code) SELECT $1,$2,$3,$4,code FROM migration_admitted_activity_manifest_issue WHERE manifest_id=$5 AND organization_id=$4").bind(new_id).bind(plan).bind(id).bind(ctx.organization_id.0).bind(old_id).execute(&mut *conn).await?;
                sqlx::query("UPDATE migration_admitted_activity_plan SET counts=$3,max_added_byte_bound=GREATEST(max_added_byte_bound,$4) WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).bind(json!(counts)).bind(added).execute(&mut *conn).await?;
            }
            _ => unreachable!(),
        }
        sqlx::query("UPDATE migration_admitted_activity_plan SET checkpoint_id=$3 WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).bind(old_id).execute(&mut *conn).await?;
    } else if phase == "manifests" {
        sqlx::query("UPDATE migration_admitted_activity_plan SET state='ready',phase='ready',checkpoint_id=NULL,completed_at=now() WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).execute(&mut *conn).await?;
        sqlx::query("UPDATE migration_admitted_activity_import SET state='queued',phase='records',counts=$3 WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).bind(p.get::<Value,_>("counts")).execute(&mut *conn).await?;
    } else {
        let next = if phase == "copying_choices" {
            "captures"
        } else {
            "manifests"
        };
        sqlx::query("UPDATE migration_admitted_activity_plan SET phase=$3,checkpoint_id=NULL WHERE id=$1 AND organization_id=$2").bind(plan).bind(ctx.organization_id.0).bind(next).execute(&mut *conn).await?;
    }
    sqlx::query("UPDATE migration_admitted_activity_import SET revision=revision+1,updated_at=now() WHERE id=$1 AND organization_id=$2").bind(id).bind(ctx.organization_id.0).execute(&mut *conn).await?;
    s::settle(conn, ctx.organization_id, id, token, 0).await
}

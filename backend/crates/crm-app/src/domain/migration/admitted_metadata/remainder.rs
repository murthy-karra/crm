//! Exact successors copy confirmed evidence in bounded transactions. Catalog
//! results already committed remain references to their original owner.
use super::super::admitted_metadata_worker as w;
use super::*;
use sqlx::{postgres::PgRow, PgConnection};

pub async fn create(
    pool: &PgPool,
    key: &RawPayloadKey,
    release: Option<&ReleaseReadiness>,
    ctx: &CommandContext,
    predecessor: Uuid,
    cmd: Request,
) -> Result<Value, MigrationError> {
    let mut tx = lifecycle_tx(pool, ctx).await?;
    let digest = request_digest(key, ctx, "remainder", predecessor, &cmd)?;
    if let Some(value) = replay_receipt(
        &mut tx,
        key,
        ctx,
        "remainder",
        cmd.request_id,
        &digest,
        predecessor,
    )
    .await?
    {
        return Ok(value);
    }
    release
        .ok_or(MigrationError::ReleaseNotReady)?
        .require_admitted_metadata(&mut tx)
        .await?;
    let r=sqlx::query("SELECT * FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(predecessor).bind(ctx.organization_id.0).fetch_optional(&mut *tx).await?.ok_or(MigrationError::NotFound)?;
    if r.get::<String, _>("state") != "cancelled"
        || r.get::<Option<Uuid>, _>("confirmed_plan_id").is_none()
    {
        return Err(MigrationError::Conflict);
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization o JOIN migration_workspace w ON w.organization_id=o.id JOIN migration_import i ON i.id=w.import_id AND i.organization_id=o.id AND i.confirmed_plan_id=w.plan_id WHERE o.id=$1 AND o.workspace_mode='migration_review' AND o.workspace_revision=$2 AND i.id=$3 AND w.plan_id=$4 AND i.state='completed')").bind(ctx.organization_id.0).bind(r.get::<i64,_>("workspace_revision")).bind(r.get::<Uuid,_>("parent_import_id")).bind(r.get::<Uuid,_>("parent_plan_id")).fetch_one(&mut *tx).await?;
    if !valid {
        return Err(MigrationError::SourceNotEligible);
    }
    let root = if let Some(root) = r.get::<Option<Uuid>, _>("successor_import_id") {
        root
    } else {
        let header = view::detail(&mut tx, ctx.organization_id, predecessor).await?;
        if header["remainder"]["available"] != true {
            return Err(MigrationError::Conflict);
        }
        let previous = sqlx::query(
            "SELECT * FROM migration_admitted_metadata_plan WHERE id=$1 AND organization_id=$2",
        )
        .bind(r.get::<Uuid, _>("confirmed_plan_id"))
        .bind(ctx.organization_id.0)
        .fetch_one(&mut *tx)
        .await?;
        let base = if previous.get::<String, _>("state") == "building" {
            previous
                .get::<Option<Uuid>, _>("remainder_plan_id")
                .ok_or(MigrationError::Conflict)?
        } else {
            previous.get("id")
        };
        let base_row = sqlx::query(
            "SELECT * FROM migration_admitted_metadata_plan WHERE id=$1 AND organization_id=$2",
        )
        .bind(base)
        .bind(ctx.organization_id.0)
        .fetch_one(&mut *tx)
        .await?;
        let root = Uuid::new_v4();
        let plan = Uuid::new_v4();
        let snapshot = r.get::<Uuid, _>("snapshot_id");
        sqlx::query("INSERT INTO migration_admitted_metadata_import(id,organization_id,parent_import_id,parent_plan_id,admission_id,predecessor_import_id,source_report_id,snapshot_id,source_account_id,capture_sequence,workspace_revision,executor_user_id,engine_version,state,phase,admission_plan_id,settled_people,counts,settled_eligible_people,held_settled_people,confirmed_at) SELECT $3,organization_id,parent_import_id,parent_plan_id,admission_id,id,source_report_id,snapshot_id,source_account_id,capture_sequence,workspace_revision,$4,engine_version,'queued','preparation',admission_plan_id,settled_people,counts,settled_eligible_people,held_settled_people,clock_timestamp() FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(predecessor).bind(ctx.organization_id.0).bind(root).bind(ctx.actor_user_id.0).execute(&mut *tx).await?;
        let inputs = seal(
            key,
            ctx.organization_id,
            snapshot,
            plan,
            plan,
            "inputs",
            &json!({"predecessor_import_id":predecessor,"confirmed_plan_id":base,"confirmation_digest":base_row.get::<Vec<u8>,_>("digest")}),
        )?;
        sqlx::query("INSERT INTO migration_admitted_metadata_plan(id,import_id,organization_id,revision,state,inputs_nonce,inputs_ciphertext,counts,snapshot_id,source_report_id,source_output_revision,capture_sequence,remainder_plan_id,evidence_plan_id,preparation_phase) VALUES($1,$2,$3,1,'building',$4,$5,'{}',$6,$7,$8,$9,$10,$11,'remainder_mappings')").bind(plan).bind(root).bind(ctx.organization_id.0).bind(inputs.nonce).bind(inputs.ciphertext).bind(snapshot).bind(r.get::<Uuid,_>("source_report_id")).bind(base_row.get::<Uuid,_>("source_output_revision")).bind(r.get::<i64,_>("capture_sequence")).bind(base).bind(base_row.get::<Option<Uuid>,_>("evidence_plan_id").unwrap_or(base)).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_metadata_import SET latest_plan_id=$3,confirmed_plan_id=$3 WHERE id=$1 AND organization_id=$2").bind(root).bind(ctx.organization_id.0).bind(plan).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_metadata_import SET successor_import_id=$3 WHERE id=$1 AND organization_id=$2").bind(predecessor).bind(ctx.organization_id.0).bind(root).execute(&mut *tx).await?;
        charge_plan(&mut tx, ctx.organization_id, root, plan, snapshot).await?;
        let count_bytes:i32=sqlx::query_scalar("SELECT octet_length(counts::text) FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(root).bind(ctx.organization_id.0).fetch_one(&mut *tx).await?;
        let token = w::reserve(
            &mut tx,
            ctx.organization_id,
            root,
            plan,
            snapshot,
            "prepare",
            i64::from(count_bytes),
        )
        .await?;
        w::settle(
            &mut tx,
            ctx.organization_id,
            root,
            token,
            i64::from(count_bytes),
        )
        .await?;
        w::reserve(
            &mut tx,
            ctx.organization_id,
            root,
            plan,
            snapshot,
            "cancel",
            metadata_store::CANCEL_RESERVATION,
        )
        .await?;
        root
    };
    let mut value = json!({"import":view::detail(&mut tx,ctx.organization_id,root).await?});
    save_receipt(
        &mut tx,
        key,
        ctx,
        "remainder",
        cmd.request_id,
        &digest,
        root,
        &mut value,
    )
    .await?;
    tx.commit().await?;
    Ok(value)
}

pub(super) async fn step(
    c: &mut PgConnection,
    key: &RawPayloadKey,
    org: OrganizationId,
    root: Uuid,
    plan: Uuid,
    p: &PgRow,
) -> Result<(), MigrationError> {
    let base: Uuid = p.get("remainder_plan_id");
    let snapshot: Uuid = p.get("snapshot_id");
    let after = p
        .get::<Option<Uuid>, _>("preparation_key")
        .unwrap_or(Uuid::nil());
    match p.get::<String, _>("preparation_phase").as_str() {
        "remainder_mappings" => {
            let previous_kind = p.get::<String, _>("preparation_kind");
            let old=sqlx::query("SELECT m.*,COALESCE(m.dependency_result_id,r.id) AS retained_result FROM migration_admitted_metadata_mapping m LEFT JOIN migration_admitted_metadata_result r ON r.import_id=m.import_id AND r.organization_id=m.organization_id AND r.unit_id=m.id WHERE m.plan_id=$1 AND m.organization_id=$2 AND (m.kind,m.id)>($3,$4) ORDER BY m.kind,m.id LIMIT 1").bind(base).bind(org.0).bind(previous_kind).bind(after).fetch_optional(&mut *c).await?;
            let Some(old) = old else {
                return phase(c, plan, "remainder_people").await;
            };
            let old_id: Uuid = old.get("id");
            let id = Uuid::new_v4();
            let f: FrozenMapping = w::open(
                key,
                org,
                snapshot,
                base,
                old_id,
                "mapping",
                &old.get::<Vec<u8>, _>("nonce"),
                &old.get::<Vec<u8>, _>("ciphertext"),
            )?;
            let encrypted = seal(key, org, snapshot, plan, id, "mapping", &f)?;
            let parent=match old.get::<Option<Uuid>,_>("parent_mapping_id"){Some(parent)=>Some(sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND predecessor_mapping_id=$3").bind(plan).bind(org.0).bind(parent).fetch_one(&mut *c).await?),None=>None};
            let dependency: Option<Uuid> = old.get("retained_result");
            let execute = old.get::<bool, _>("execute_unit")
                && dependency.is_none()
                && matches!(
                    old.get::<String, _>("disposition").as_str(),
                    "create_matching" | "map_existing"
                );
            sqlx::query("INSERT INTO migration_admitted_metadata_mapping(id,import_id,plan_id,organization_id,kind,source_key,source_id,parent_mapping_id,target_id,target_field_id,disposition,nonce,ciphertext,predecessor_mapping_id,source_mapping_id,dependency_result_id,execute_unit,alias_count) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)").bind(id).bind(root).bind(plan).bind(org.0).bind(old.get::<String,_>("kind")).bind(old.get::<Vec<u8>,_>("source_key")).bind(old.get::<String,_>("source_id")).bind(parent).bind(old.get::<Option<Uuid>,_>("target_id")).bind(old.get::<Option<Uuid>,_>("target_field_id")).bind(old.get::<String,_>("disposition")).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(old_id).bind(old.get::<Option<Uuid>,_>("source_mapping_id").unwrap_or(old_id)).bind(dependency).bind(execute).bind(old.get::<i64,_>("alias_count")).execute(&mut *c).await?;
            sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_key=$2,preparation_kind=$3,fields_processed=fields_processed+1 WHERE id=$1").bind(plan).bind(old_id).bind(old.get::<String,_>("kind")).execute(c).await?;
        }
        "remainder_people" => {
            let old=sqlx::query("SELECT * FROM migration_admitted_metadata_manifest WHERE plan_id=$1 AND organization_id=$2 AND disposition='cancelled' AND id>$3 ORDER BY id LIMIT 1").bind(base).bind(org.0).bind(after).fetch_optional(&mut *c).await?;
            let Some(old) = old else {
                return phase(c, plan, "seal").await;
            };
            let old_id: Uuid = old.get("id");
            let id = Uuid::new_v4();
            let baseline: Value = w::open(
                key,
                org,
                snapshot,
                base,
                old_id,
                "baseline",
                &old.get::<Vec<u8>, _>("baseline_nonce"),
                &old.get::<Vec<u8>, _>("baseline_ciphertext"),
            )?;
            let encrypted = seal(key, org, snapshot, plan, id, "baseline", &baseline)?;
            sqlx::query("INSERT INTO migration_admitted_metadata_manifest(id,import_id,plan_id,organization_id,admission_result_id,person_id,source_person_id,disposition,baseline_nonce,baseline_ciphertext,item_byte_bound,expected_person_id,predecessor_manifest_id) VALUES($1,$2,$3,$4,$5,$6,$7,'eligible',$8,$9,$10,$11,$12)").bind(id).bind(root).bind(plan).bind(org.0).bind(old.get::<Uuid,_>("admission_result_id")).bind(old.get::<Option<Uuid>,_>("person_id")).bind(old.get::<String,_>("source_person_id")).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(old.get::<i64,_>("item_byte_bound")).bind(old.get::<Option<Uuid>,_>("expected_person_id")).bind(old_id).execute(&mut *c).await?;
            sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_key=$2,preparation_manifest=$3,preparation_parent=NULL,preparation_phase='remainder_operations',people_processed=people_processed+1 WHERE id=$1").bind(plan).bind(old_id).bind(id).execute(c).await?;
        }
        "remainder_operations" => {
            let old_manifest: Uuid = p.get("preparation_key");
            let manifest: Uuid = p.get("preparation_manifest");
            let rows=sqlx::query("SELECT * FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND organization_id=$2 AND id>$3 ORDER BY id LIMIT 50").bind(old_manifest).bind(org.0).bind(p.get::<Option<Uuid>,_>("preparation_parent").unwrap_or(Uuid::nil())).fetch_all(&mut *c).await?;
            if rows.is_empty() {
                sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_phase='remainder_people',preparation_parent=NULL WHERE id=$1").bind(plan).execute(c).await?;
                return Ok(());
            }
            for old in rows {
                let old_id: Uuid = old.get("id");
                let id = Uuid::new_v4();
                let frozen: FrozenOperation = w::open(
                    key,
                    org,
                    snapshot,
                    base,
                    old_id,
                    "operation",
                    &old.get::<Vec<u8>, _>("nonce"),
                    &old.get::<Vec<u8>, _>("ciphertext"),
                )?;
                let encrypted = seal(key, org, snapshot, plan, id, "operation", &frozen)?;
                let mapping=match old.get::<Option<Uuid>,_>("mapping_id"){Some(mapping)=>Some(sqlx::query_scalar::<_,Uuid>("SELECT id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND predecessor_mapping_id=$3").bind(plan).bind(org.0).bind(mapping).fetch_one(&mut *c).await?),None=>None};
                sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,mapping_id,source_key,target_id,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(id).bind(manifest).bind(root).bind(plan).bind(org.0).bind(old.get::<String,_>("kind")).bind(mapping).bind(old.get::<Vec<u8>,_>("source_key")).bind(old.get::<Option<Uuid>,_>("target_id")).bind(old.get::<String,_>("disposition")).bind(encrypted.nonce).bind(encrypted.ciphertext).execute(&mut *c).await?;
                sqlx::query(
                    "UPDATE migration_admitted_metadata_plan SET preparation_parent=$2 WHERE id=$1",
                )
                .bind(plan)
                .bind(old_id)
                .execute(&mut *c)
                .await?;
            }
        }
        _ => return Err(MigrationError::Conflict),
    }
    Ok(())
}
async fn phase(c: &mut PgConnection, plan: Uuid, phase: &str) -> Result<(), MigrationError> {
    sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_phase=$2,preparation_kind='',preparation_key=NULL,preparation_parent=NULL WHERE id=$1").bind(plan).bind(phase).execute(c).await?;
    Ok(())
}
